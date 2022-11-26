use crate::trsfr::proxy::Proxy;
use crate::lb::backend::loadbalancer::LoadBalancer;
use crate::trsfr::serve::ConnectMethod::ConnectMethod;

use async_trait::async_trait;
use log::{debug, error};

use std::sync::Arc;
use std::net::SocketAddr;
use std::error::Error;

use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::try_join;

#[async_trait]
impl ConnectMethod for Proxy {
    fn listener(&self) -> SocketAddr {
        self.listener.clone()
    }

    async fn run_server<LB: LoadBalancer + 'static>(
        &self,
        backend: Arc<RwLock<LB>>,
    ) -> Result<(), Box<dyn Error>> {

        let listener_addr = self.listener();
        debug!("Listening on: {:?}", listener_addr.ip());
        let listener = TcpListener::bind(listener_addr).await?;

        while let Ok((inbound, _)) = listener.accept().await {
            // clones for async thread
            let thread_backend = Arc::clone(&backend);
            tokio::spawn(async move {
                if let Err(e) = process(inbound, thread_backend).await {
                    error!("Failed to process tcp request; error={}", e);
                }
            });
        }
        Ok(())
    }
}

async fn process<LB: LoadBalancer>(
    mut inbound: TcpStream,
    thread_backend: Arc<RwLock<LB>>,
) -> Result<(), Box<dyn Error>> {
    let mut srvrs = thread_backend.write().await;

    if let Some(server_addr) = srvrs.get() {
        let mut server = TcpStream::connect(&server_addr).await?;

        let (mut ri, mut wi) = inbound.split();
        let (mut ro, mut wo) = server.split();

        let client_to_server = io::copy(&mut ri, &mut wo);
        let server_to_client = io::copy(&mut ro, &mut wi);

        let (bytes_tx, bytes_rx): (u64, u64) = try_join!(client_to_server, server_to_client)?;

        debug!(
            "client wrote {:?} bytes and received {:?} bytes",
            bytes_tx, bytes_rx
        );
    } else {
        error!("Unable to process request")
    }
    Ok(())
}
