mod lb;

use redis::RedisError;
use std::collections::HashMap;
use std::net::SocketAddr;


pub struct RoundRobinServer {
    addr: SocketAddr,
}

pub struct RoundRobinBackend {
    backends: HashMap<SocketAddr, RoundRobinServer>,
    addresses: Vec<SocketAddr>,
    last_used: usize,
}

impl RoundRobinBackend {
    pub fn new() -> Self {
        RoundRobinBackend {
            backends: HashMap::new(),
            addresses: Vec::new(),
            last_used: 0,
        }
    }
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

impl redis::FromRedisValue for RoundRobinServer {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<RoundRobinServer> {
        if let redis::Value::Data(ref bytes) = *v {
            match std::str::from_utf8(bytes)?.parse() {
                Ok(addr) => Ok(RoundRobinServer { addr }),
                Err(e) => Err(redis_error!(v, e.to_string())),
            }
        } else {
            Err(redis_error!(v, "Response type not string compatible."))
        }
    }
}
