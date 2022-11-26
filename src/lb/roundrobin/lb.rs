use crate::lb::roundrobin::{RoundRobinBackend, RoundRobinServer};
use crate::lb::backend::loadbalancer::LoadBalancer;

use std::error::Error;
use std::slice::Iter;
use std::net::SocketAddr;

struct KeyError { key: String }

impl LoadBalancer for RoundRobinBackend {
    type Server = RoundRobinServer;
    fn get(&mut self) -> Option<SocketAddr> {
        self.last_used = (self.last_used + 1) % self.backends.len();
        match self.addresses.get(self.last_used) {
            Some(server) => Some(server.to_owned()),
            None => None,
        }
    }
    fn add(&mut self, balance_server: RoundRobinServer) -> Result<(), Box<dyn Error>> {
        self.addresses.push(balance_server.addr);
        self.backends.insert(balance_server.addr, balance_server);
        Ok(())
    }

    fn remove(&mut self, balance_server: RoundRobinServer) -> Result<(), Box<dyn Error>> {
        self.addresses.retain(|&x| x != balance_server.addr);
        match self.backends.remove(&balance_server.addr) {
            Some(_) => Ok(()),
            None => Err(Box::new(
                std::io::Error::new(std::io::ErrorKind::NotFound,
                format!("Key `{}` not found and I have no better error, this is fine", balance_server.addr)
            ))) // This is my low, making the mistake of Box<dyn Error> and getting punsihed for laziness
        }
    }
    fn mark_living(&mut self, _address: SocketAddr) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn mark_dead(&mut self, address: SocketAddr) -> Result<(), Box<dyn Error>> {
        if let Some(index) = self.addresses.iter().position(|&value| value == address) {
            self.addresses.swap_remove(index);
            self.backends.remove(&address);
        };
        Ok(())
    }

    fn iter<'a>(&'a self) -> Iter<'a, SocketAddr> {
        self.addresses.iter()
    }
}
