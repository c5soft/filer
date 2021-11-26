use crate::JsonHelper;
use serde_json::Value;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr}; //, ToSocketAddrs};

pub(crate) struct Addr {
    ipv4: Ipv4Addr,
    port: u16,
    is_active:bool,
    is_https: bool,
}
impl Addr {
    pub fn new(config: &Value, is_https: bool) -> Self {
        Addr {
            ipv4: "0.0.0.0".parse().unwrap(), //Ipv4Addr::new(127, 0, 0, 1),
            port: config[if is_https { "https_port" } else { "http_port" }].u64(80) as u16,
            is_active: config[if is_https { "https_active" } else { "http_active" }].bool(false),
            is_https,
        }
    }
    pub fn get(&self) -> (bool, SocketAddr) {
        (
            self.is_active,
            SocketAddr::new(IpAddr::V4(self.ipv4), self.port),
        )
    }
    // pub fn to_string_full(&self) -> String {
    //     format!("{}:{}", self.ipv4, self.port)
    // }
}


impl From<Addr> for SocketAddr {
    fn from(a:Addr)->SocketAddr {
        SocketAddr::new(IpAddr::V4(a.ipv4), a.port)
    }
}

// impl Into<SocketAddr> for Addr {
//     fn into(self)->SocketAddr {
//         SocketAddr::new(IpAddr::V4(self.ipv4), self.port)
//     }
// }

// impl ToSocketAddrs for Addr {
//     type Iter = std::vec::IntoIter<SocketAddr>;
//     fn to_socket_addrs(&self) -> std::io::Result<Self::Iter> {
//         Ok(vec![SocketAddr::new(IpAddr::V4(self.ipv4), self.port)].into_iter())
//     }
// }

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let protocol = if self.is_https { "https" } else { "http" };
        let ip = {
            let ip = format!("{}", self.ipv4);
            if ip == "0.0.0.0" {
                "127.0.0.1".to_owned()
            } else {
                ip
            }
        };
        let port = if (self.is_https && self.port == 443_u16)
            || (!self.is_https && self.port == 80_u16)
        {
            String::from("")
        } else {
            format!(":{}", self.port)
        };
        write!(f, "{}://{}{}", protocol, ip, port)
    }
}
