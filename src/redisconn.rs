use crate::lb::backend::loadbalancer::LoadBalancer;
use log::debug;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;

static REDIS_CHANNELS: [&str; 2] = ["backend_add", "backend_remove"];

pub mod redisconn {
    use crate::lb::backend::loadbalancer::LoadBalancer;
    use crate::redisconn::{handle_message, subscribe};

    use log::error;

    use std::sync::Arc;

    use futures::stream::StreamExt;
    use tokio::sync::RwLock;

    pub fn get_redis_client(url: &str) -> Result<Arc<redis::Client>, redis::RedisError> {
        match redis::Client::open(url) {
            Ok(client) => Ok(Arc::new(client)),
            Err(e) => Err(e),
        }
    }

    pub async fn redis_pubsub_loop<LB: LoadBalancer + 'static>(
        redis_client: Arc<redis::Client>,
        backend: Arc<RwLock<LB>>,
    ) -> redis::RedisResult<()> {
        let conn = redis_client.get_async_connection().await.unwrap();
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

async fn subscribe(conn: redis::aio::Connection) -> redis::RedisResult<redis::aio::PubSub> {
    let mut pubsub = conn.into_pubsub();
    for channel in REDIS_CHANNELS {
        pubsub.subscribe(channel).await?;
        debug!("Channel {:?} subscribed to", channel);
    }
    Ok(pubsub)
}

fn extract_msg<'a, LB: LoadBalancer>(
    msg: &'a redis::Msg,
) -> redis::RedisResult<(&'a str, LB::Server)> {
    Ok((msg.get_channel_name(), msg.get_payload::<LB::Server>()?))
}

async fn handle_message<LB: LoadBalancer + 'static>(
    //TODO add logging and clean up code
    backend: Arc<RwLock<LB>>,
    msg: &redis::Msg,
) -> Result<(), Box<dyn Error>> {
    match extract_msg::<LB>(msg) {
        Ok(("backend_add", payload)) => {
            let mut srvs = backend.write().await;
            match srvs.add(payload) {
                Ok(_) => {
                    println!("added");
                    debug!("New Server added from {:?}", msg);
                }
                _ => {println!("error");}
            }
        }
        Ok(("backend_remove", payload)) => {
            let mut srvs = backend.write().await;
            match srvs.remove(payload) {
                Ok(_) => {
                    println!("Removed");
                    debug!("Server Removed added from {:?}", msg);
                }
                _ => {println!("error2");}
            }
        }
        _ => (),
    }
    Ok(())
}
