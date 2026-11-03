//! Wireless debugging: `adb pair`, `adb connect`/`disconnect`, mDNS discovery, and the paired-hosts list adb keeps
//! in `~/.android/adb_known_hosts.pb`.

use crate::adb::Adb;
use crate::error::{DeviceError, Result};
use std::path::{Path, PathBuf};

/// A service seen by `adb mdns services`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsService {
    /// Instance name, e.g. `adb-R58M123-AbCdEf`.
    pub name: String,
    /// What the phone offers on this port.
    pub kind: MdnsKind,
    /// `ip:port`.
    pub address: String,
}

/// mDNS service types adb knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdnsKind {
    /// `_adb-tls-pairing._tcp`: the "Pair device with pairing code" screen is open.
    Pairing,
    /// `_adb-tls-connect._tcp`: a paired phone with wireless debugging on.
    Connect,
    /// `_adb._tcp`: legacy `adb tcpip`.
    Legacy,
    Other(String),
}

/// Parses `adb mdns services` (tab-separated `name  type  ip:port`).
pub fn parse_mdns_services(text: &str) -> Vec<MdnsService> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t').map(str::trim).filter(|s| !s.is_empty());
            let (name, ty, address) = (parts.next()?, parts.next()?, parts.next()?);
            let kind = match ty.trim_end_matches('.') {
                "_adb-tls-pairing._tcp" => MdnsKind::Pairing,
                "_adb-tls-connect._tcp" => MdnsKind::Connect,
                "_adb._tcp" => MdnsKind::Legacy,
                other => MdnsKind::Other(other.to_string()),
            };
            Some(MdnsService { name: name.to_string(), kind, address: address.to_string() })
        })
        .collect()
}

impl Adb {
    /// `adb pair host:port code` (Android 11+ "Pair device with pairing code").
    pub fn pair(&self, address: &str, code: &str) -> Result<String> {
        let out = self.run(&["pair", address, code])?;
        let text = format!("{}{}", out.stdout, out.stderr);
        if out.code == 0 && text.contains("Successfully paired") {
            Ok(text.trim().to_string())
        } else {
            Err(DeviceError::PairFailed(text.trim().to_string()))
        }
    }

    /// `adb connect host:port`; returns the serial to use.
    pub fn connect(&self, address: &str) -> Result<String> {
        let out = self.run(&["connect", address])?;
        let text = format!("{}{}", out.stdout, out.stderr);
        let lower = text.to_ascii_lowercase();
        if lower.contains("connected to") && !lower.contains("failed") && !lower.contains("cannot") {
            Ok(address.to_string())
        } else {
            Err(DeviceError::ConnectFailed(text.trim().to_string()))
        }
    }

    /// `adb disconnect host:port`.
    pub fn disconnect(&self, address: &str) -> Result<()> {
        self.run(&["disconnect", address]).map(drop)
    }

    /// Whether adb's mDNS discovery works (`adb mdns check`).
    pub fn mdns_available(&self) -> Result<bool> {
        let out = self.run(&["mdns", "check"])?;
        Ok(out.code == 0 && out.stdout.contains("mdns daemon version"))
    }

    /// `adb mdns services`.
    pub fn mdns_services(&self) -> Result<Vec<MdnsService>> {
        let out = self.run(&["mdns", "services"])?;
        if out.code != 0 {
            return Err(DeviceError::Command { code: out.code, message: out.stderr.trim().to_string() });
        }
        Ok(parse_mdns_services(&out.stdout))
    }
}

/// adb's user directory: `$ANDROID_USER_HOME`, else `~/.android`.
pub fn android_user_dir(env: &dyn Fn(&str) -> Option<std::ffi::OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    env("ANDROID_USER_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| home.map(|h| h.join(".android")))
}

/// The paired-hosts file below adb's user dir.
pub fn known_hosts_file(android_dir: &Path) -> PathBuf {
    android_dir.join("adb_known_hosts.pb")
}

fn varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *data.get(*pos)?;
        *pos += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// Length-delimited field-1 payloads of a protobuf message (other fields skipped); `None` when malformed.
fn field1_strings(data: &[u8]) -> Option<Vec<&[u8]>> {
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < data.len() {
        let key = varint(data, &mut pos)?;
        match key & 7 {
            0 => {
                varint(data, &mut pos)?;
            }
            2 => {
                let len = usize::try_from(varint(data, &mut pos)?).ok()?;
                let body = data.get(pos..pos.checked_add(len)?)?;
                pos += len;
                if key >> 3 == 1 {
                    out.push(body);
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Parses `AdbKnownHosts { repeated HostInfo { string guid = 1; } = 1 }` into the guids.
pub fn parse_known_hosts(data: &[u8]) -> Option<Vec<String>> {
    field1_strings(data)?
        .into_iter()
        .map(|host| {
            let guid = field1_strings(host)?.into_iter().next().unwrap_or_default();
            Some(String::from_utf8_lossy(guid).into_owned())
        })
        .collect()
}

/// Encodes guids as `AdbKnownHosts`.
pub fn encode_known_hosts(guids: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for g in guids {
        let mut host = vec![0x0a];
        put_varint(&mut host, g.len() as u64);
        host.extend_from_slice(g.as_bytes());
        out.push(0x0a);
        put_varint(&mut out, host.len() as u64);
        out.extend(host);
    }
    out
}

/// The paired hosts (guids such as `adb-R58M123-AbCdEf`); empty when the file does not exist.
pub fn paired_hosts(android_dir: &Path) -> Result<Vec<String>> {
    match std::fs::read(known_hosts_file(android_dir)) {
        Ok(data) => parse_known_hosts(&data)
            .ok_or_else(|| DeviceError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad known hosts"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

/// Removes a paired host; returns whether it was there. Restart the adb server afterwards
/// ([`Adb::kill_server`]) so it forgets the pairing too.
pub fn forget_paired_host(android_dir: &Path, guid: &str) -> Result<bool> {
    let mut hosts = paired_hosts(android_dir)?;
    let before = hosts.len();
    hosts.retain(|h| h != guid);
    if hosts.len() == before {
        return Ok(false);
    }
    std::fs::write(known_hosts_file(android_dir), encode_known_hosts(&hosts))?;
    Ok(true)
}
