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
        fn get(&mut self) -> Option<SocketAddr>;
        fn add(&mut self, balance_server: Self::Server) -> Result<(), Box<dyn Error>>;
        fn remove(&mut self, balance_server: Self::Server) -> Result<(), Box<dyn Error>>;
        fn mark_living(&mut self, address: SocketAddr) -> Result<(), Box<dyn Error>>;
        fn mark_dead(&mut self, address: SocketAddr) -> Result<(), Box<dyn Error>>;
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
        ) -> Result<(), Box<dyn Error>>
        where
            F: FnMut(Arc<RwLock<LB>>) -> Fut,
            Fut: Future<Output = Result<(), Box<dyn Error>>>,
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
) -> Result<(), Box<dyn std::error::Error>> {
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
                    fn get(&mut self) -> Option<SocketAddr>;
                    fn add(&mut self, balance_server: MockServer) -> Result<(), Box<dyn Error>>;
                    fn remove(&mut self, balance_server: MockServer) -> Result<(), Box<dyn Error>>;
                    fn mark_living(&mut self, address: SocketAddr) ->  Result<(), Box<dyn Error>>;
                    fn mark_dead(&mut self, address: SocketAddr) -> Result<(), Box<dyn Error>>;
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

    #[tokio::test]
    async fn test_update_backend_health() {
        MockSetup!();
        let m_lb = MockLB::new();
        let mut m_pool = serverpool::Pool::new("test_name".to_string(), m_lb, 1_usize);
        let mut i = 0;

        let mock_check_health = |_: Arc<RwLock<MockLB>>| {
            i += 1;
            async move {if i < 3 {Ok(())} else {Err("Err".into())}}
        };

        let start = Instant::now();
        let e = m_pool.update_health_loop(mock_check_health).await;
        assert_eq!(
            e.err().unwrap().to_string(),
            <&str as Into<Box<dyn Error>>>::into("Err").to_string()
        );
        assert!(start.elapsed() >= Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_check_backend_health() {
        MockSetup!();
        let ars = vec![
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
        ];
        let mut m_lb = MockLB::new();

        m_lb.expect_iter().return_const(ars.into_iter());
        m_lb.expect_mark_dead()
            .times(2)
            .returning(|_: SocketAddr| Ok(()));

        let m_pool = serverpool::Pool::new("test_name".to_string(), m_lb, 1_usize);

        assert!(check_backend_health(m_pool.backend).await.is_ok())
    }
}
