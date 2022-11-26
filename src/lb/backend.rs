use log::debug;

use std::error::Error;
use std::net::{SocketAddr, TcpStream};

use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::sync::RwLock;

pub mod loadbalancer {
    use std::error::Error;
    use std::net::SocketAddr;
    use std::slice::Iter;
    pub trait LoadBalancer: Send + Sync {
        type Server: redis::FromRedisValue + Send;
        fn get(&mut self) -> Option<SocketAddr>;
        fn add(&mut self, balance_server: Self::Server) -> Result<(), Box<dyn Error>>;
        fn remove(&mut self, balance_server: Self::Server) -> Result<(), Box<dyn Error>>;
        fn mark_living(&mut self, address: SocketAddr) -> Result<(), Box<dyn Error>>;
        fn mark_dead(&mut self, address: SocketAddr) -> Result<(), Box<dyn Error>>;
        fn iter(&self) -> Iter<'_, SocketAddr>;
    }
}

pub mod serverpool {
    use super::loadbalancer::LoadBalancer;
    use super::update_backend_health;

    use log::debug;

    use std::error::Error;
    use std::time::Duration;

    use std::sync::Arc;

    use tokio::sync::RwLock;

    pub struct Pool<LB: LoadBalancer> {
        pub name: String,
        pub backend: Arc<RwLock<LB>>,
        pub health_check_interval: usize,
    }

    impl<LB: LoadBalancer + 'static> Pool<LB> {
        pub fn new(name: String, servers: LB, health_check_interval: usize) -> Self {
            Pool {
                name,
                backend: Arc::new(RwLock::new(servers)),
                health_check_interval,
            }
        }

        pub async fn update_health_loop(&mut self) -> Result<(), Box<dyn Error>> {
            let mut time_interval =
                tokio::time::interval(Duration::from_secs(self.health_check_interval as u64)
            );
            loop {
                debug!("Running backend health checker{:?}", time_interval);
                if let Err(e) = update_backend_health(self.backend.clone()).await {
                    return Err(e);
                }
                time_interval.tick().await;
            }
        }
    }
}

async fn update_backend_health<LB: loadbalancer::LoadBalancer + 'static>(
    backend: Arc<RwLock<LB>>,
) -> Result<(), Box<dyn Error>> {
    let mut thread_socket_vec: Vec<(thread::JoinHandle<Status>, SocketAddr)> = Vec::new();
    let mut srvs = backend.write().await;

    for address in srvs.iter() {
        let moved_addr = address.to_owned();
        let join = thread::spawn(move || health_check_tcp(moved_addr));
        thread_socket_vec.push((join, address.clone()));
    }

    for (status, address) in thread_socket_vec {
        match status.join() {
            Ok(Status::Alive) => srvs.mark_living(address),
            Ok(Status::Dead) | Err(_) => srvs.mark_dead(address),
        };
    }
    Ok(())
}

enum Status {
    Alive,
    Dead,
}

fn health_check_tcp(address: SocketAddr) -> Status {
    match TcpStream::connect_timeout(&address, Duration::from_secs(3)) {
        Ok(_) => Status::Alive,
        Err(_) => {
            debug!("Server {} has been marked dead", { address });
            Status::Dead
        }
    }
}
