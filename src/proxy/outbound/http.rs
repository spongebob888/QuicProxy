use anyhow::Result;
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{error, info};

pub struct HttpOutbound {
    address: String,
    port: u16,
}

impl HttpOutbound {
    pub fn new(address: String, port: u16) -> Self {
        Self { address, port }
    }

    pub async fn listen(&self) -> Result<(), Box<dyn Error>> {
        let addr = format!("{}:{}", self.address, self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("HTTP Inbound listening on {}", addr);

        loop {
            let (mut socket, _) = listener.accept().await?;
            tokio::spawn(async move {
                let mut buf = [0; 1024];
                match socket.read(&mut buf).await {
                    Ok(n) if n == 0 => return,
                    Ok(n) => {
                        info!("Received {} bytes", n);
                        // 简单的 echo，后续会替换为真正的代理逻辑
                        if let Err(e) = socket.write_all(&buf[0..n]).await {
                            error!("Failed to write to socket: {}", e);
                        }
                    }
                    Err(e) => error!("Failed to read from socket: {}", e),
                }
            });
        }
    }
}
