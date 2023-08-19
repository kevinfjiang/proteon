use log::debug;
use std::clone::Clone;
use std::error::Error;
use std::iter::Iterator;
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::thread;

use std::time::Duration;
use tokio::sync::RwLock;

pub mod loadbalancer {
    use super::*;
    pub trait LoadBalancer: Send + Sync {
        type Server: redis::FromRedisValue + Send;
        type It: Iterator<Item = SocketAddr>;
        type Err: Error;

        fn get(&mut self) -> Option<SocketAddr>;
        fn add(&mut self, balance_server: Self::Server) -> Result<(), Self::Err>;
        fn remove(&mut self, balance_server: Self::Server) -> Result<(), Self::Err>;
        fn mark_living(&mut self, address: SocketAddr) -> Result<(), Self::Err>;
        fn mark_dead(&mut self, address: SocketAddr) -> Result<(), Self::Err>;
        fn iter(&self) -> Self::It;
    }
}

pub mod serverpool {
    use super::{loadbalancer::LoadBalancer, *};
    use futures::Future;

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

        pub async fn update_health_loop<F, Fut>(
            &mut self,
            mut async_check: F,
        ) -> Result<(), LB::Err>
        where
            F: FnMut(Arc<RwLock<LB>>) -> Fut,
            Fut: Future<Output = Result<(), LB::Err>>,
        {
            let mut time_interval =
                tokio::time::interval(Duration::from_secs(self.health_check_interval as u64));
            loop {
                debug!("Running backend health checker{:?}", time_interval);
                async_check(self.backend.clone()).await?;
                time_interval.tick().await;
            }
        }
    }
}

pub async fn check_backend_health<LB: loadbalancer::LoadBalancer + 'static>(
    backend: Arc<RwLock<LB>>,
) -> Result<(), LB::Err> {
    let mut thread_socket_vec: Vec<(thread::JoinHandle<Status>, SocketAddr)> = Vec::new();
    let mut srvs = backend.write().await;

    for address in srvs.iter() {
        let moved_addr = address.to_owned();
        let join = thread::spawn(move || health_check_tcp(moved_addr));
        thread_socket_vec.push((join, address));
    }

    for (status, address) in thread_socket_vec {
        match status.join() {
            Ok(Status::Alive) => srvs.mark_living(address)?,
            Ok(Status::Dead) | Err(_) => srvs.mark_dead(address)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::error::Error;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::{Duration, Instant};
    use std::vec::IntoIter;

    use mockall::*;
    use redis::RedisError;

    #[derive(Debug, PartialEq, Eq)]
    pub struct MockServer {
        addr: SocketAddr,
    }

    macro_rules! redis_error {
        ($v:expr, $det:expr) => {
            RedisError::from((
                redis::ErrorKind::TypeError,
                "Response was of incompatible type",
                format!("{:?} (response was {:?})", $det, $v),
            ))
        };
    }

    impl redis::FromRedisValue for MockServer {
        fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
            if let redis::Value::Data(ref bytes) = *v {
                match std::str::from_utf8(bytes)?.parse() {
                    Ok(addr) => Ok(MockServer { addr }),
                    Err(e) => Err(redis_error!(v, e.to_string())),
                }
            } else {
                Err(redis_error!(v, "Response type not string compatible."))
            }
        }
    }

    macro_rules! MockSetup {
        () => {
            mock! {
                LB {}
                impl loadbalancer::LoadBalancer for LB{
                    type Server = MockServer;
                    type It = IntoIter<SocketAddr>;
                    type Err = std::io::Error;
                    fn get(&mut self) -> Option<SocketAddr>;
                    fn add(&mut self, balance_server: MockServer) -> Result<(), std::io::Error>;
                    fn remove(&mut self, balance_server: MockServer) -> Result<(), std::io::Error>;
                    fn mark_living(&mut self, address: SocketAddr) ->  Result<(),  std::io::Error>;
                    fn mark_dead(&mut self, address: SocketAddr) -> Result<(),  std::io::Error>;
                    fn iter(&self) -> IntoIter<SocketAddr>;
                }
            }
        };
    }

    #[test]
    fn test_new_pool() {
        MockSetup!();
        let m_lb = MockLB::new();
        let m_pool = serverpool::Pool::new("test_name".to_string(), m_lb, 1_usize);
        assert_eq!(m_pool.name, "test_name".to_string());
        assert_eq!(m_pool.health_check_interval, 1_usize);
    }

}
