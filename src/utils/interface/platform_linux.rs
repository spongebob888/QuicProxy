use super::InterfaceInfo;
use crate::utils::new_io_other_error;
use std::io;
use std::net::IpAddr;
use std::process::Command;

pub(super) struct ListContext;

impl ListContext {
    pub(super) fn new() -> Self {
        Self
    }
}

pub(super) fn enhance_interface(
    _ctx: &ListContext,
    _iface_name: &str,
    _friendly_name: &mut Option<String>,
    _gateway: &mut Option<String>,
) {
}

pub(super) fn set_dns(iface: &InterfaceInfo, dns: &[IpAddr]) -> io::Result<()> {
    let dns_strings: Vec<String> = dns.iter().map(|ip| ip.to_string()).collect();
    let interface_name = &iface.name;

    if Command::new("resolvectl").arg("--version").output().is_ok() {
        let mut args = vec!["dns", interface_name];
        for dns in &dns_strings {
            args.push(dns);
        }
        let output = Command::new("resolvectl").args(&args).output()?;
        if !output.status.success() {
            return Err(new_io_other_error(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        return Ok(());
    }

    if Command::new("nmcli").arg("--version").output().is_ok() {
        let dns_joined = dns_strings.join(" ");
        let output = Command::new("nmcli")
            .args(&["dev", "modify", interface_name, "ipv4.dns", &dns_joined])
            .output()?;
        if !output.status.success() {
            return Err(new_io_other_error(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        return Ok(());
    }

    Err(new_io_other_error(
        "No supported DNS management tool found (resolvectl, nmcli)",
    ))
}

pub(super) fn get_dns(iface: &InterfaceInfo) -> io::Result<Vec<IpAddr>> {
    let interface_name = &iface.name;
    if Command::new("resolvectl").arg("--version").output().is_ok() {
        let output = Command::new("resolvectl")
            .args(&["dns", interface_name])
            .output()?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let mut dns_servers = Vec::new();
            if let Some(idx) = output_str.find(':') {
                let ips_str = &output_str[idx + 1..];
                for part in ips_str.split_whitespace() {
                    if let Ok(ip) = part.parse::<IpAddr>() {
                        dns_servers.push(ip);
                    }
                }
            }
            return Ok(dns_servers);
        }
    }

    if Command::new("nmcli").arg("--version").output().is_ok() {
        let output = Command::new("nmcli")
            .args(&["dev", "show", interface_name])
            .output()?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let mut dns_servers = Vec::new();
            for line in output_str.lines() {
                if line.contains("IP4.DNS") {
                    if let Some(idx) = line.find(':') {
                        let val = line[idx + 1..].trim();
                        if let Ok(ip) = val.parse::<IpAddr>() {
                            dns_servers.push(ip);
                        }
                    }
                }
            }
            return Ok(dns_servers);
        }
    }

    Err(new_io_other_error(
        "No supported DNS management tool found (resolvectl, nmcli)",
    ))
}

pub(super) fn restore_dns(iface: &InterfaceInfo) -> io::Result<()> {
    let interface_name = &iface.name;
    if Command::new("resolvectl").arg("--version").output().is_ok() {
        let output = Command::new("resolvectl")
            .args(&["revert", interface_name])
            .output()?;
        if !output.status.success() {
            return Err(new_io_other_error(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        return Ok(());
    }

    if Command::new("nmcli").arg("--version").output().is_ok() {
        let output = Command::new("nmcli")
            .args(&["dev", "modify", interface_name, "ipv4.dns", ""])
            .output()?;
        if !output.status.success() {
            return Err(new_io_other_error(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        return Ok(());
    }

    Err(new_io_other_error(
        "No supported DNS management tool found (resolvectl, nmcli)",
    ))
}
