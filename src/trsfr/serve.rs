use log::error;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::RwLock;

pub mod transfermethod {
    use super::*;
    use crate::lb::backend::loadbalancer::LoadBalancer;

    use async_trait::async_trait;
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum BalanceError<LB: LoadBalancer> {
        #[error("Load balance error as follows {0}")]
        LoadBalanceError(LB::Err),
        #[error("Io error regarding awaiting a lock {0}")]
        IoError(std::io::Error)
    }

    impl<LB: LoadBalancer> From<std::io::Error> for BalanceError<LB>{
        fn from(err: std::io::Error) -> BalanceError<LB>{
            BalanceError::IoError::<LB>(err)
        }

    }

    #[async_trait]
    pub trait TransferMethod: Send + Sync {
        fn listener(&self) -> SocketAddr;
        async fn run_server<LB: LoadBalancer + 'static>(
            &self,
            backend: Arc<RwLock<LB>>,
        ) -> Result<(), BalanceError<LB>>;
    }
}

pub mod server {
    use super::transfermethod::TransferMethod;
    use super::*;

    use crate::lb::backend::{check_backend_health, loadbalancer::LoadBalancer, serverpool::Pool};
    use crate::redisconn::dbconn::redis_pubsub_loop;

    macro_rules! async_loop_if_err {
        ($x:expr, $msg: expr) => {
            if let Err(e) = ($x).await {
                error!(concat!($msg, ": {}"), e)
            };
        };
    }

    #[tokio::main]
    pub async fn start_server<LB: LoadBalancer + 'static, CM: TransferMethod + 'static>(
        mut pool: Pool<LB>,
        connect: CM,
        redis_client: Arc<redis::Client>,
    ) -> Result<(), LB::Err> {
        let m_backend = Arc::clone(&pool.backend);
        let backend = Arc::clone(&pool.backend);

        tokio::spawn(async move {
            async_loop_if_err!(
                redis_pubsub_loop(redis_client.clone(), m_backend),
                "Redis Listen loop died"
            );
        });
        tokio::spawn(async move {
            // We will have to decouple these
            async_loop_if_err!(
                pool.update_health_loop(check_backend_health),
                "Error checking healthy servers"
            );
        });

        async_loop_if_err!(connect.run_server(backend), "Load balancing server died");

        Ok(())
    }
}
