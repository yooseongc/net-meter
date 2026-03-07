use std::sync::Arc;

use net_meter_core::TcpPayload;
use net_meter_metrics::Collector;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing::debug;

/// TCP 서버 스폰: accept 루프 + 연결당 핸들러 태스크
pub fn spawn_tcp_server(
    listener: TcpListener,
    payload: TcpPayload,
    global: Arc<Collector>,
    proto: Arc<Collector>,
) -> JoinHandle<()> {
    let client_tx = payload.tx_bytes; // 클라이언트가 보내는 바이트 (서버가 읽을 양)
    let server_tx = payload.rx_bytes; // 서버가 응답할 바이트 수
    tokio::spawn(async move {
        loop {
            let (stream, _peer) = match listener.accept().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(error = %e, "TCP accept error");
                    continue;
                }
            };
            let g = Arc::clone(&global);
            let p = Arc::clone(&proto);
            tokio::spawn(async move {
                handle_conn(stream, client_tx, server_tx, g, p).await;
            });
        }
    })
}

/// 단일 TCP 연결 처리.
///
/// - `client_tx > 0`: 클라이언트가 보내는 데이터를 round마다 정확히 `client_tx` 바이트 읽는다.
/// - `client_tx == 0`: 데이터를 한 청크 읽은 후 응답 (파악 불가한 스트림 처리).
/// - `server_tx > 0`: 매 round 후 `server_tx` 바이트 응답을 전송한다.
/// - `server_tx == 0`: 응답 없음 (단방향 수신).
async fn handle_conn(
    mut stream: tokio::net::TcpStream,
    client_tx: usize,
    server_tx: usize,
    global: Arc<Collector>,
    proto: Arc<Collector>,
) {
    global.record_server_request(0);
    proto.record_server_request(0);

    let response = if server_tx > 0 { vec![0u8; server_tx] } else { vec![] };
    let mut buf = vec![0u8; client_tx.max(4096)];

    loop {
        if client_tx > 0 {
            // 정확히 client_tx 바이트를 읽어야 한 round로 간주
            let mut received = 0;
            while received < client_tx {
                match stream.read(&mut buf[received..client_tx]).await {
                    Ok(0) => return, // EOF
                    Ok(n) => received += n,
                    Err(_) => return,
                }
            }
        } else {
            // 길이 미지정: 데이터가 올 때마다 한 청크 읽기
            match stream.read(&mut buf).await {
                Ok(0) => return,
                Ok(_) => {}
                Err(_) => return,
            }
        }

        if !response.is_empty() {
            if stream.write_all(&response).await.is_err() {
                return;
            }
        }
    }
}
