use crate::lb::backend::loadbalancer::LoadBalancer;
use log::{debug, error};
use std::sync::Arc;

use tokio::sync::RwLock;
use redis::{self, aio};

use std::error::Error;

static REDIS_CHANNELS: [&str; 2] = ["backend_add", "backend_remove"];

pub mod dbconn {
    use super::*;

    use crate::lb::backend::loadbalancer::LoadBalancer;
    use crate::redisconn::{handle_message, subscribe};

    use futures::StreamExt;


    pub fn get_redis_client(url: &str) -> Result<Arc<redis::Client>,redis::RedisError> {
        match redis::Client::open(url) {
            Ok(client) => Ok(Arc::new(client)),
            Err(e) => Err(e),
        }
    }

    pub async fn redis_pubsub_loop<LB: LoadBalancer + 'static>(
        redis_client: Arc<redis::Client>,
        backend: Arc<RwLock<LB>>,
    ) -> redis::RedisResult<()> {
        let conn = redis_client.get_async_connection().await?;
        let pubsub = subscribe(conn).await.unwrap();
        let mut msg_stream = pubsub.into_on_message();

        while let Some(msg) = msg_stream.next().await {
            match handle_message(Arc::clone(&backend), &msg).await {
                Ok(()) => (),
                Err(x) => {
                    error!("Error raised with {:?}", x)
                }
            };
        }
        Ok(())
    }
}

async fn subscribe(conn: aio::Connection) -> redis::RedisResult<aio::PubSub> {
    let mut pubsub = conn.into_pubsub();
    for channel in REDIS_CHANNELS {
        pubsub.subscribe(channel).await?;
        debug!("Channel {:?} subscribed to", channel);
    }
    Ok(pubsub)
}

fn extract_msg<LB: LoadBalancer>(
    msg: & redis:: Msg,
) -> redis::RedisResult<(&str, LB::Server)> {
    Ok((msg.get_channel_name(), msg.get_payload::<LB::Server>()?))
}

async fn handle_message<LB: LoadBalancer + 'static>(
    //TODO add logging and clean up code
    backend: Arc<RwLock<LB>>,
    msg: &redis::Msg,
) -> Result<(), LB::Err> {
    match extract_msg::<LB>(msg) {
        Ok(("backend_add", payload)) => {
            let mut srvs = backend.write().await;
            if srvs.add(payload).is_ok() {
                debug!("New Server added from {:?}", msg);
            }
        }
        Ok(("backend_remove", payload)) => {
            let mut srvs = backend.write().await;
            if srvs.remove(payload).is_ok() {
                debug!("Server Removed added from {:?}", msg);
            }
        }
        _ => (),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::SocketAddr;
    use std::net::{IpAddr, Ipv4Addr};
    use std::vec::IntoIter;

    use mockall::*;
    use predicate::*;

    macro_rules! MockSetup {
        () => {
            mock! {
                LB {}
                impl LoadBalancer for LB{
                    type Server = MockServer;
                    type It = IntoIter<SocketAddr>;
                    type Err = std::io::Error;
                    fn get(&mut self) -> Option<SocketAddr>;
                    fn add(&mut self, balance_server: MockServer) -> Result<(), std::io::Error>;
                    fn remove(&mut self, balance_server: MockServer) -> Result<(), std::io::Error>;
                    fn mark_living(&mut self, address: SocketAddr) ->  Result<(), std::io::Error>;
                    fn mark_dead(&mut self, address: SocketAddr) -> Result<(), std::io::Error>;
                    fn iter(&self) -> IntoIter<SocketAddr>;
                }
            }
        };
    }

    macro_rules! to_redis_msg {
        ($($x:expr),+ $(,)?) => {
            redis::Value::Bulk(
                vec!(
                    redis::Value::Data(b"message".to_vec()),
                    $(redis::Value::Data(($x).to_vec()),)*
                )
            )
        };
    }
    #[derive(Debug, PartialEq, Eq)]
    pub struct MockServer {
        addr: SocketAddr,
    }

    macro_rules! redis_error {
        ($v:expr, $det:expr) => {
            redis:: RedisError::from((
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

    #[tokio::test]
    async fn test_handle_message_live() {
        MockSetup!();
        let m_lb = MockLB::new();
        let ref_mlb = Arc::new(RwLock::new(m_lb));
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let rr = MockServer { addr };
        {
            ref_mlb
                .write()
                .await
                .expect_add()
                .with(predicate::eq(rr))
                .times(1)
                .returning(|_: MockServer| Ok(()));
        }

        let value1 = to_redis_msg!(b"backend_add", addr.to_string().as_bytes());
        let message1 =redis::Msg::from_value(&value1).unwrap();
        assert!(handle_message(ref_mlb, &message1).await.is_ok());
    }

    #[tokio::test]
    async fn test_handle_message_dead() {
        MockSetup!();
        let m_lb = MockLB::new();
        let ref_mlb = Arc::new(RwLock::new(m_lb));
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let rr = MockServer { addr };
        {
            ref_mlb
                .write()
                .await
                .expect_remove()
                .with(predicate::eq(rr))
                .times(1)
                .returning(|_: MockServer| Ok(()));
        }

        let value1 = to_redis_msg!(b"backend_remove", addr.to_string().as_bytes());
        let message1 = redis::Msg::from_value(&value1).unwrap();
        assert!(handle_message(ref_mlb, &message1).await.is_ok());
    }

    #[tokio::test]
    async fn test_handle_message_err() {
        MockSetup!();
        let m_lb = MockLB::new();
        let ref_mlb = Arc::new(RwLock::new(m_lb));

        let value1 = to_redis_msg!(b"backend_add", b"nonesense_string");
        let message1 = redis::Msg::from_value(&value1).unwrap();
        assert!(handle_message(ref_mlb, &message1).await.is_ok());
    }

}
