use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use rtp_rs::RtpReader;
use rand::prelude::*; // Thread-güvenli RNG için
use tonic::{transport::Server, Request, Response, Status};

pub mod media {
    tonic::include_proto!("media");
}
use media::media_manager_server::{MediaManager, MediaManagerServer};
use media::{AllocatePortRequest, AllocatePortResponse};

const HOST: &str = "0.0.0.0";
const RTP_MIN_PORT: u16 = 10000;
const RTP_MAX_PORT: u16 = 20000;
const GRPC_PORT: u16 = 50052;

type ActiveSessions = Arc<Mutex<Vec<u16>>>;

#[derive(Debug, Default)]
pub struct MyMediaManager {
    active_sessions: ActiveSessions,
}

#[tonic::async_trait]
impl MediaManager for MyMediaManager {
    async fn allocate_port(
        &self,
        _request: Request<AllocatePortRequest>,
    ) -> Result<Response<AllocatePortResponse>, Status> {
        println!("\n[gRPC] AllocatePort isteği alındı...");

        // bind_and_listen_rtp artık thread-güvenli
        let port = bind_and_listen_rtp(self.active_sessions.clone()).await
            .map_err(|e| Status::internal(format!("RTP portu atanamadı: {}", e)))?;

        println!("[gRPC] Yeni RTP portu atandı: {}", port);
        
        let reply = AllocatePortResponse { port: port as u32 };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let active_sessions = Arc::new(Mutex::new(Vec::new()));
    let addr = format!("0.0.0.0:{}", GRPC_PORT).parse()?;
    let manager = MyMediaManager {
        active_sessions: active_sessions.clone(),
    };
    let grpc_server = Server::builder()
        .add_service(MediaManagerServer::new(manager))
        .serve(addr);

    println!("✅ [Media Server - gRPC] Başarıyla başlatıldı, dinleniyor: {}", addr);
    tokio::spawn(grpc_server);

    println!("💡 [Media Server] Geliştirme modu aktif. Ctrl+C ile kapatabilirsiniz.");
    tokio::signal::ctrl_c().await?;
    println!("🛑 Sunucu kapatılıyor...");
    Ok(())
}

async fn bind_and_listen_rtp(active_sessions: ActiveSessions) -> Result<u16, std::io::Error> {
    // --- DÜZELTME BURADA ---
    // `thread_rng()` yerine, tüm thread'lerden erişilebilen `SmallRng` kullanıyoruz.
    // `from_entropy()` ile her seferinde rastgele bir başlangıç noktası (seed) alırız.
    let mut rng = SmallRng::from_entropy();
    // ----------------------

    for _ in 0..100 {
        let port = rng.gen_range(RTP_MIN_PORT..=RTP_MAX_PORT);
        let addr_str = format!("{}:{}", HOST, port);
        
        if let Ok(socket) = UdpSocket::bind(&addr_str).await {
            active_sessions.lock().unwrap().push(port);
            println!("[RTP] Yeni dinleyici başlatıldı: {}", addr_str);
            tokio::spawn(rtp_packet_listener(socket));
            return Ok(port);
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "Boş port bulunamadı"))
}

async fn rtp_packet_listener(sock: UdpSocket) {
    let mut buf = [0u8; 2048];
    loop {
        // 'addr' değişkenini kullanmadığımız için adını '_addr' olarak değiştiriyoruz.
        if let Ok((_len, _addr)) = sock.recv_from(&mut buf).await { // DÜZELTME
            match RtpReader::new(&buf[.._len]) {
                Ok(rtp) => {
                    println!(
                        "🚀 [RTP Port {}] Paket alındı! (Sıra No: {:?}, SSRC: {:?})",
                        sock.local_addr().unwrap().port(),
                        rtp.sequence_number(),
                        rtp.ssrc()
                    );
                },
                Err(_) => {} // Hatalı paketleri şimdilik görmezden gel
            }
        }
    }
}