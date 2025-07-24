use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::interval;
use rand::prelude::*;
use tonic::{transport::Server, Request, Response, Status};
// `hound`'u doğrudan kullanacağımız için spesifik import'a gerek yok.
use config::{Config, File};
use serde::Deserialize;
use tracing::{info, error, instrument, Level};
use tracing_subscriber::FmtSubscriber;

pub mod media { tonic::include_proto!("media"); }
use media::media_manager_server::{MediaManager, MediaManagerServer};
use media::{AllocatePortRequest, AllocatePortResponse};

#[derive(Debug, Deserialize, Clone)]
struct GrpcConfig { host: String, port: u16, }
#[derive(Debug, Deserialize, Clone)]
struct RtpConfig { host: String, min_port: u16, max_port: u16, }
#[derive(Debug, Deserialize, Clone)]
struct AnnouncementConfig { welcome_file_path: String, }
#[derive(Debug, Deserialize, Clone)]
struct Settings {
    grpc: GrpcConfig,
    rtp: RtpConfig,
    announcement: AnnouncementConfig,
}

type ActiveSessions = Arc<Mutex<Vec<u16>>>;

#[derive(Debug)]
pub struct MyMediaManager {
    active_sessions: ActiveSessions,
    settings: Arc<Settings>,
}

#[tonic::async_trait]
impl MediaManager for MyMediaManager {
    #[instrument(skip(self))]
    async fn allocate_port(&self, _request: Request<AllocatePortRequest>) -> Result<Response<AllocatePortResponse>, Status> {
        info!("AllocatePort isteği alındı...");
        let (port, sock) = bind_rtp_port(&self.settings.rtp).await
            .map_err(|e| { error!(error = %e, "RTP portu atanamadı"); Status::internal("RTP portu atanamadı") })?;
        
        let shared_sock = Arc::new(sock);
        tokio::spawn(rtp_session_handler(shared_sock, self.active_sessions.clone(), port, self.settings.clone()));

        info!(rtp_port = port, "Yeni RTP portu atandı");
        let reply = AllocatePortResponse { port: port as u32 };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber)?;
    let settings = Config::builder()
        .add_source(File::with_name("config/default"))
        .build()?
        .try_deserialize::<Settings>()?;
    info!(config = ?settings, "Konfigürasyon yüklendi");

    let active_sessions = Arc::new(Mutex::new(Vec::new()));
    let addr = format!("{}:{}", settings.grpc.host, settings.grpc.port).parse()?;
    let manager = MyMediaManager {
        active_sessions,
        settings: Arc::new(settings),
    };
    let grpc_server = Server::builder().add_service(MediaManagerServer::new(manager)).serve(addr);

    info!(address = %addr, "gRPC sunucusu başlatılıyor...");
    tokio::spawn(grpc_server);

    tokio::signal::ctrl_c().await?;
    info!("Sunucu kapatılıyor...");
    Ok(())
}

async fn bind_rtp_port(rtp_config: &RtpConfig) -> Result<(u16, UdpSocket), std::io::Error> {
    let mut rng = SmallRng::from_entropy();
    for _ in 0..100 {
        let port = rng.gen_range(rtp_config.min_port..=rtp_config.max_port);
        let addr_str = format!("{}:{}", rtp_config.host, port);
        if let Ok(socket) = UdpSocket::bind(&addr_str).await {
            return Ok((port, socket));
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "Boş port bulunamadı"))
}

async fn rtp_session_handler(sock: Arc<UdpSocket>, active_sessions: ActiveSessions, port: u16, settings: Arc<Settings>) {
    active_sessions.lock().unwrap().push(port);
    info!(rtp_port = port, "Yeni RTP oturumu için dinleyici başlatıldı");

    let mut remote_addr: Option<std::net::SocketAddr> = None;
    let mut buf = [0u8; 2048];

    loop {
        if let Ok((_len, addr)) = sock.recv_from(&mut buf).await {
            if remote_addr.is_none() {
                info!(remote = %addr, rtp_port = port, "İlk RTP paketi alındı, ses gönderimi başlıyor...");
                remote_addr = Some(addr);
                
                let sock_clone = Arc::clone(&sock);
                tokio::spawn(send_welcome_announcement(sock_clone, addr, settings.clone()));
            }
        }
    }
}

async fn send_welcome_announcement(sock: Arc<UdpSocket>, target_addr: std::net::SocketAddr, settings: Arc<Settings>) {
    let file_path = &settings.announcement.welcome_file_path;
    let reader = match hound::WavReader::open(file_path) {
        Ok(r) => r,
        Err(e) => { error!(file = %file_path, error = %e, "WAV dosyası açılamadı"); return; }
    };
    
    let samples_per_packet = 160;
    let mut interval = interval(Duration::from_millis(20));
    let ssrc: u32 = rand::thread_rng().gen();
    let mut sequence_number: u16 = rand::thread_rng().gen();
    let mut zaman_damgasi: u32 = rand::thread_rng().gen();
    let payload_type: u8 = 0; // PCMU

    let samples: Vec<u8> = reader.into_samples::<i8>().map(|s| s.unwrap() as u8).collect();

    for chunk in samples.chunks(samples_per_packet) {
        interval.tick().await;

        let mut rtp_packet = Vec::with_capacity(12 + chunk.len());
        rtp_packet.push(0x80);
        rtp_packet.push(payload_type);
        rtp_packet.extend_from_slice(&sequence_number.to_be_bytes());
        rtp_packet.extend_from_slice(&zaman_damgasi.to_be_bytes());
        rtp_packet.extend_from_slice(&ssrc.to_be_bytes());
        rtp_packet.extend_from_slice(chunk);

        if let Err(e) = sock.send_to(&rtp_packet, target_addr).await {
            error!("RTP paketi gönderilemedi: {}", e);
            break;
        }
        
        sequence_number = sequence_number.wrapping_add(1);
        zaman_damgasi = zaman_damgasi.wrapping_add(samples_per_packet as u32);
    }
    info!(remote = %target_addr, file = %file_path, "Anons gönderimi tamamlandı.");
}