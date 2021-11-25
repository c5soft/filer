use super::JsonHelper;
use serde_json::Value;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub(crate) struct Addr {
    ipv4: Ipv4Addr,
    port: u16,
    https_active: bool,
}
impl Addr {
    pub fn new(config: &Value, is_https: bool) -> Self {
        Addr {
            ipv4: "0.0.0.0".parse().unwrap(), //Ipv4Addr::new(127, 0, 0, 1),
            port: config[if is_https { "https_port" } else { "http_port" }].u64(80) as u16,
            https_active: is_https,
        }
    }
    pub fn parse(&self) -> (bool, SocketAddr) {
        (
            self.https_active,
            SocketAddr::new(IpAddr::V4(self.ipv4), self.port),
        )
    }
}
impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let protocol = if self.https_active { "https" } else { "http" };
        let ip = {
            let ip = format!("{}", self.ipv4);
            if ip == "0.0.0.0" {
                "127.0.0.1".to_owned()
            } else {
                ip
            }
        };
        let port = if (self.https_active && self.port == 443_u16)
            || (!self.https_active && self.port == 80_u16)
        {
            String::from("")
        } else {
            format!(":{}", self.port)
        };
        write!(f, "{}://{}{}", protocol, ip, port)
    }
}
