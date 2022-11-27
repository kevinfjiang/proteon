extern crate clap;
extern crate redis;
extern crate tokio;

mod lb;
mod redisconn;
mod trsfr;

use std::{env, net::SocketAddr};

fn main() {
    let pool_name = env::var("POOL_NAME").unwrap();
    let redis_url = env::var("REDIS_URL").unwrap();
    let listening_port: SocketAddr = env::var("LISTENING_PORT").unwrap().parse().unwrap();

    let rrb = lb::roundrobin::RoundRobinBackend::new();
    let cm = trsfr::proxy::Proxy::new(listening_port);
    let c = redisconn::dbconn::get_redis_client(&redis_url).unwrap();
    let pool = lb::backend::serverpool::Pool::new(pool_name, rrb, 10);

    trsfr::serve::server::start_server(pool, cm, c).unwrap();
}
