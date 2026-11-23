use std::fs;
use std::path::Path;

use crate::error::{AppError, AppResult};

pub fn find_pid_listening_on_port(port: u16) -> AppResult<Option<u32>> {
    let mut inodes = Vec::new();
    let mut any_table_read = false;
    let mut last_err = None;

    for table_path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        match fs::read_to_string(table_path) {
            Ok(content) => {
                any_table_read = true;
                for inode in parse_listening_inodes(&content, port) {
                    if !inodes.contains(&inode) {
                        inodes.push(inode);
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => last_err = Some(format!("read {table_path} failed: {err}")),
        }
    }

    if !any_table_read {
        if let Some(err) = last_err {
            return Err(AppError::Message(err));
        }
        return Ok(None);
    }

    if inodes.is_empty() {
        return Ok(None);
    }
    pid_for_socket_inodes(&inodes)
}

pub(crate) fn parse_listening_inodes(content: &str, port: u16) -> Vec<String> {
    let port_hex = format!("{port:04X}");
    let mut inodes = Vec::new();
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 {
            continue;
        }
        let local = parts[1];
        let state = parts[3];
        if state != "0A" {
            continue;
        }
        let Some((_, port_part)) = local.split_once(':') else {
            continue;
        };
        if port_part.eq_ignore_ascii_case(&port_hex) {
            let inode = parts[9];
            if inode != "0" && !inodes.iter().any(|existing| existing == inode) {
                inodes.push(inode.to_string());
            }
        }
    }
    inodes
}

fn pid_for_socket_inodes(inodes: &[String]) -> AppResult<Option<u32>> {
    let proc = Path::new("/proc");
    let entries =
        fs::read_dir(proc).map_err(|err| AppError::Message(format!("read /proc failed: {err}")))?;

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid_str) = file_name.to_str() else {
            continue;
        };
        if !pid_str.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        let fd_dir = entry.path().join("fd");
        let Ok(fds) = fs::read_dir(fd_dir) else {
            continue;
        };
        for fd in fds.flatten() {
            let Ok(link) = fs::read_link(fd.path()) else {
                continue;
            };
            let link_str = link.to_string_lossy();
            if link_str.starts_with("socket:[") && link_str.ends_with(']') {
                let link_inode = &link_str[8..link_str.len() - 1];
                if inodes.iter().any(|wanted| wanted == link_inode) {
                    return Ok(Some(pid));
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::parse_listening_inodes;

    #[test]
    fn parses_ipv4_and_ipv6_listen_socket_inodes_and_skips_established() {
        let tcp4 = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:705D 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 42101 1 0000000000000000 100 0 0 10 0\n\
   1: 0100007F:705D 0100007F:C350 01 00000000:00000000 00:00000000 00000000  1000        0 99999 1 0000000000000000 20 4 30 10 -1\n";
        let tcp6 = "\
  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 00000000000000000000000000000000:705D 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 88412 1 0000000000000000 100 0 0 10 0\n";

        assert_eq!(parse_listening_inodes(tcp4, 28765), vec!["42101"]);
        assert_eq!(parse_listening_inodes(tcp6, 28765), vec!["88412"]);
        assert!(parse_listening_inodes(tcp4, 8080).is_empty());
    }
}
