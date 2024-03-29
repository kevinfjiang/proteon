use crate::lb::backend::loadbalancer::LoadBalancer;
use crate::lb::roundrobin::{RoundRobinBackend, RoundRobinServer};

use thiserror::Error;
use std::net::SocketAddr;
use std::vec::IntoIter;

#[derive(Error, Debug)]
pub enum RoundRobinError {
    #[error("the data for key `{0}` is not available")]
    KeyNotFound(String)
}

impl LoadBalancer for RoundRobinBackend {
    type Server = RoundRobinServer;
    type It = IntoIter<SocketAddr>;
    type Err = RoundRobinError;
    fn get(&mut self) -> Option<SocketAddr> {
        self.last_used = (self.last_used + 1) % self.backends.len();
        self.addresses.get(self.last_used).map(|server| server.to_owned())
    }
    fn add(&mut self, balance_server: RoundRobinServer) -> Result<(), Self::Err> {
        self.addresses.push(balance_server.addr);
        self.backends.insert(balance_server.addr, balance_server);
        Ok(())
    }

    fn remove(&mut self, balance_server: RoundRobinServer) -> Result<(), Self::Err> {
        self.addresses.retain(|&x| x != balance_server.addr);
        match self.backends.remove(&balance_server.addr) {
            Some(_) => Ok(()),
            None => Err(RoundRobinError::KeyNotFound(balance_server.addr.to_string())),
        }
    }
    fn mark_living(&mut self, _address: SocketAddr) -> Result<(), Self::Err> {
        Ok(())
    }
    fn mark_dead(&mut self, address: SocketAddr) -> Result<(), Self::Err> {
        if let Some(index) = self.addresses.iter().position(|&value| value == address) {
            self.addresses.swap_remove(index);
            self.backends.remove(&address);
        };
        Ok(())
    }

    fn iter(&self) -> IntoIter<SocketAddr> {
        self.addresses.clone().into_iter()
    }
}
