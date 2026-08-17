use std::net::SocketAddr;

pub fn bind_addr(port: u16, allow_lan_access: bool) -> SocketAddr {
    let host = if allow_lan_access {
        [0, 0, 0, 0]
    } else {
        [127, 0, 0, 1]
    };
    SocketAddr::from((host, port))
}

pub fn bind_host(allow_lan_access: bool) -> &'static str {
    if allow_lan_access {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_access_is_the_default() {
        assert_eq!(bind_addr(28766, false), SocketAddr::from(([127, 0, 0, 1], 28766)));
    }

    #[test]
    fn lan_access_binds_all_interfaces() {
        assert_eq!(bind_addr(28766, true), SocketAddr::from(([0, 0, 0, 0], 28766)));
    }
}
