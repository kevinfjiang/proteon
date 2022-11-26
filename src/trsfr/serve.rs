pub mod ConnectMethod {
    use crate::lb::backend::loadbalancer::LoadBalancer;

    use async_trait::async_trait;

    use std::error::Error;

    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[async_trait]
    pub trait ConnectMethod: Send + Sync {
        fn listener(&self) -> SocketAddr;
        async fn run_server<LB: LoadBalancer + 'static>(
            &self,
            backend: Arc<RwLock<LB>>,
        ) -> Result<(), Box<dyn Error>>;
    }
}

pub mod Server {
    use super::ConnectMethod::ConnectMethod;
    use crate::lb::backend::{loadbalancer::LoadBalancer, serverpool::Pool};
    use crate::redisconn::redisconn::redis_pubsub_loop;

    use log::error;

    use std::error::Error;
    use std::sync::Arc;

    macro_rules! async_loop_if_err {
        ($x:expr, $msg: expr) => {
            if let Err(e) = ($x).await {
                error!(concat!($msg, ": {}"), e)
            };
        }
    }



    #[tokio::main]
    pub async fn start_server<LB: LoadBalancer + 'static, CM: ConnectMethod + 'static>(
        mut pool: Pool<LB>,
        connect: CM,
        redis_client: Arc<redis::Client>,
    ) -> Result<(), Box<dyn Error>> {
        let m_backend = Arc::clone(&pool.backend);
        let backend = Arc::clone(&pool.backend);


        tokio::spawn(async move {
            async_loop_if_err!(redis_pubsub_loop(redis_client.clone(), m_backend), "Redis Listen loop died");
        });
        tokio::spawn(async move {
            async_loop_if_err!(pool.update_health_loop(), "Error checking healthy servers");
        });

        async_loop_if_err!(connect.run_server(backend), "Load balancing server died");

        Ok(())
    }
}


