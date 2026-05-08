use crate::config::OutboundConfig;
use crate::dns::get_dns_by_tag;
use crate::proxy::outbound::{AnyOutbound, AnyPacket, AnyStream};
use crate::proxy::{SessionCloser, SourceAddr, TargetAddr};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tracing::{Instrument, debug, error};

pub struct DnsOutbound {
    tag: String,
    dns: String,
    connect_timeout: Duration,
}

impl DnsOutbound {
    pub fn new(tag: String, cfg: &OutboundConfig) -> Result<Arc<dyn AnyOutbound>> {
        let dns = cfg
            .dns
            .clone()
            .context(format!("dns outbound '{}' requires dns", tag))?;

        let connect_timeout = Duration::from_secs(cfg.connect_timeout.unwrap_or(30));

        Ok(Arc::new(Self {
            tag,
            dns,
            connect_timeout,
        }))
    }
}

struct DnsUdpOutbound {
    rx: Mutex<mpsc::Receiver<(TargetAddr, TargetAddr, Bytes)>>,
    tx: mpsc::Sender<(TargetAddr, TargetAddr, Bytes)>,
    dns: String,
    closer: Arc<SessionCloser>,
}

#[async_trait]
impl AnyPacket for DnsUdpOutbound {
    fn closer(&self) -> Arc<SessionCloser> {
        self.closer.clone()
    }

    async fn send_to(&self, buf: Bytes, target: &TargetAddr, from: &SourceAddr) -> Result<usize> {
        let tx = self.tx.clone();
        let closer = self.closer.clone();
        let target = target.clone();
        let from = from.clone();
        let len = buf.len();
        let dns_server = get_dns_by_tag(&self.dns);

        let current_span = tracing::Span::current();
        tokio::spawn(
            async move {
                debug!("send dns query.");
                match dns_server.hijack_exchange(&buf.to_vec()).await {
                    Ok(response) => {
                        let _ = tx.send((target, from, Bytes::from(response))).await;
                    }
                    Err(e) => {
                        error!("failed to send dns query: {}", e);
                        closer.close();
                    }
                }
            }
            .instrument(current_span),
        );

        Ok(len)
    }

    async fn recv_from(&self) -> Result<(TargetAddr, TargetAddr, Bytes)> {
        let mut rx = self.rx.lock().await;
        let closer = self.closer.clone();
        tokio::select! {
            res = rx.recv() => {
                match res {
                    Some(res) => {
                        self.closer.close();
                        Ok(res)
                    }
                    None => bail!("Channel closed"),

                }
            }
            _ = closer.wait() => {
                bail!("Channel closed");
            }
        }
    }
}

#[async_trait]
impl AnyOutbound for DnsOutbound {
    fn tag(&self) -> &str {
        &self.tag
    }

    fn protocol(&self) -> &str {
        "dns"
    }

    fn dns_server_name(&self) -> Option<&str> {
        Some(self.dns.as_str())
    }

    fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    async fn connect_stream_base(&self) -> Result<AnyStream> {
        bail!("DNS Outbound does not support TCP yet")
    }

    async fn connect_stream_with(
        &self,
        _target: &TargetAddr,
        _stream: AnyStream,
    ) -> Result<AnyStream> {
        bail!("DNS Outbound does not support TCP yet")
    }

    async fn connect_packet(&self, _target: &TargetAddr) -> Result<Arc<dyn AnyPacket>> {
        let (tx, rx) = mpsc::channel(1);
        let closer = Arc::new(SessionCloser::new());

        let inner = Arc::new(DnsUdpOutbound {
            rx: Mutex::new(rx),
            tx,
            dns: self.dns.clone(),
            closer: closer.clone(),
        });
        Ok(inner)
    }
}
