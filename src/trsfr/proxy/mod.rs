mod process;

use std::net::SocketAddr;

pub struct Proxy {
    pub listener: SocketAddr,
}

impl Proxy {
    pub fn new(listener: SocketAddr) -> Self {
        Proxy { listener }
    }
}
