use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use rtp_rs::RtpReader;
use rand::prelude::*;
use tonic::{transport::Server, Request, Response, Status};
use hound::{WavSpec, WavWriter};
use config::{Config, File};
use serde::Deserialize;

// --- YENİ VE DOĞRU IMPORT BÖLÜMÜ ---
// `tracing` kütüphanesinden ihtiyacımız olan her şeyi import ediyoruz
use tracing::{info, error, instrument, Level};
use tracing_subscriber::FmtSubscriber;
// ------------------------------------

// gRPC Bölümü
pub mod media {
    tonic::include_proto!("media");
}
use media::media_manager_server::{MediaManager, MediaManagerServer};
use media::{AllocatePortRequest, AllocatePortResponse};

// Konfigürasyon Yapısı
#[derive(Debug, Deserialize)]
struct GrpcConfig {
    host: String,
    port: u16,
}
#[derive(Debug, Deserialize)]
struct RtpConfig {
    host: String,
    min_port: u16,
    max_port: u16,
}
#[derive(Debug, Deserialize)]
struct Settings {
    grpc: GrpcConfig,
    rtp: RtpConfig,
}

// Paylaşılan Veri Yapıları
type AudioBuffers = Arc<Mutex<HashMap<u32, Vec<i16>>>>;
type ActiveSessions = Arc<Mutex<Vec<u16>>>;
const SAMPLE_RATE: u32 = 8000;

// gRPC Sunucusu
#[derive(Debug)]
pub struct MyMediaManager {
    active_sessions: ActiveSessions,
    audio_buffers: AudioBuffers,
    rtp_config: RtpConfig,
}

#[tonic::async_trait]
impl MediaManager for MyMediaManager {
    #[instrument(skip(self))]
    async fn allocate_port(
        &self,
        _request: Request<AllocatePortRequest>,
    ) -> Result<Response<AllocatePortResponse>, Status> {
        info!("AllocatePort isteği alındı...");
        let port = bind_and_listen_rtp(
            self.active_sessions.clone(),
            self.audio_buffers.clone(),
            &self.rtp_config
        ).await.map_err(|e| {
            error!(error = %e, "RTP portu atanamadı");
            Status::internal("RTP portu atanamadı")
        })?;
        info!(rtp_port = port, "Yeni RTP portu atandı");
        let reply = AllocatePortResponse { port: port as u32 };
        Ok(Response::new(reply))
    }
}

// Ana Fonksiyon
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // En basit ve en güvenilir loglama kurulumu.
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .json() // Bu satırı yorumdan çıkararak logları JSON formatında alabilirsiniz.
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let settings = Config::builder()
        .add_source(File::with_name("config/default"))
        .build()?
        .try_deserialize::<Settings>()?;
    info!(config = ?settings, "Konfigürasyon başarıyla yüklendi");

    let active_sessions = Arc::new(Mutex::new(Vec::new()));
    let audio_buffers = Arc::new(Mutex::new(HashMap::new()));
    
    let addr = format!("{}:{}", settings.grpc.host, settings.grpc.port).parse()?;
    let manager = MyMediaManager {
        active_sessions,
        audio_buffers: audio_buffers.clone(),
        rtp_config: settings.rtp,
    };
    let grpc_server = Server::builder()
        .add_service(MediaManagerServer::new(manager))
        .serve(addr);

    info!(address = %addr, "gRPC sunucusu başlatılıyor...");
    tokio::spawn(grpc_server);

    tokio::signal::ctrl_c().await?;
    
    info!("Sunucu kapatılıyor... Ses verileri WAV dosyalarına yazılıyor...");
    save_audio_buffers_to_wav(audio_buffers);
    
    Ok(())
}

#[instrument(skip(active_sessions, audio_buffers, rtp_config))]
async fn bind_and_listen_rtp(
    active_sessions: ActiveSessions,
    audio_buffers: AudioBuffers,
    rtp_config: &RtpConfig
) -> Result<u16, std::io::Error> {
    let mut rng = SmallRng::from_entropy();
    for _ in 0..100 {
        let port = rng.gen_range(rtp_config.min_port..=rtp_config.max_port);
        let addr_str = format!("{}:{}", rtp_config.host, port);
        if let Ok(socket) = UdpSocket::bind(&addr_str).await {
            active_sessions.lock().unwrap().push(port);
            info!(rtp_address = %addr_str, "Yeni RTP dinleyici başlatıldı");
            tokio::spawn(rtp_packet_listener(socket, audio_buffers.clone()));
            return Ok(port);
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "Boş port bulunamadı"))
}

#[instrument(skip(sock, audio_buffers))]
async fn rtp_packet_listener(sock: UdpSocket, audio_buffers: AudioBuffers) {
    let mut buf = [0u8; 2048];
    loop {
        if let Ok((len, _addr)) = sock.recv_from(&mut buf).await {
            if let Ok(rtp) = RtpReader::new(&buf[..len]) {
                if rtp.payload_type() == 0 {
                    let ssrc = rtp.ssrc();
                    let payload = rtp.payload();
                    let audio_samples: Vec<i16> = payload.iter().map(|&byte| g711_ulaw_to_pcm16(byte)).collect();
                    audio_buffers.lock().unwrap().entry(ssrc).or_default().extend(audio_samples);
                }
            }
        }
    }
}

fn save_audio_buffers_to_wav(audio_buffers: AudioBuffers) {
    let buffers = audio_buffers.lock().unwrap();
    if buffers.is_empty() {
        info!("Kaydedilecek ses verisi bulunamadı.");
        return;
    }
    for (ssrc, samples) in buffers.iter() {
        let filename = format!("ssrc_{}.wav", ssrc);
        info!(filename = %filename, "Ses kaydı dosyaya yazılıyor...");
        let spec = WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&filename, spec).unwrap();
        for &sample in samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }
    info!("Tüm ses kayıtları tamamlandı.");
}

fn g711_ulaw_to_pcm16(ulaw_byte: u8) -> i16 {
    let ulaw = !ulaw_byte;
    let sign = (ulaw & 0x80) as i16;
    let exponent = ((ulaw >> 4) & 0x07) as i16;
    let mantissa = (ulaw & 0x0F) as i16;
    let mut sample = (mantissa << 3) + 0x84;
    sample <<= exponent;
    if sign != 0 {
        -(sample - 0x84)
    } else {
        sample - 0x84
    }
}