//! Owner-local saved endpoints. An endpoint is a transport convenience,
//! never identity trust, a project authority, or a remote merge permission.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};

const MAX_REMOTES: usize = 64;
const MAX_CONFIG: u64 = 32_768;

fn usage(message: &str) -> CliError {
    CliError::Usage(message.into())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(Clone, Debug)]
struct Remote {
    address: String,
    peer: Option<did_mini::Did>,
}

fn load(home: &Path) -> Result<BTreeMap<String, Remote>> {
    let file = match File::open(home.join("remotes")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(CliError::Io(e.to_string())),
    };
    let mut text = String::new();
    file.take(MAX_CONFIG + 1)
        .read_to_string(&mut text)
        .map_err(|e| CliError::Io(e.to_string()))?;
    if text.len() as u64 > MAX_CONFIG {
        return Err(usage("remote configuration exceeds limit"));
    }
    let mut remotes = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if !(fields.len() == 2 || fields.len() == 3) {
            return Err(usage("corrupt remote configuration"));
        }
        let (name, addr) = (fields[0], fields[1]);
        let peer = if fields.len() == 3 {
            Some(
                did_mini::Did::parse(fields[2])
                    .map_err(|_| usage("corrupt remote peer identity"))?,
            )
        } else {
            None
        };
        if !valid_name(name)
            || addr.parse::<SocketAddr>().is_err()
            || remotes
                .insert(
                    name.to_owned(),
                    Remote {
                        address: addr.to_owned(),
                        peer,
                    },
                )
                .is_some()
            || remotes.len() > MAX_REMOTES
        {
            return Err(usage("corrupt remote configuration"));
        }
    }
    Ok(remotes)
}

pub fn dispatch(home: &Path, store: &Path, args: Vec<String>, json: bool) -> Result<String> {
    let verb = args.first().map(String::as_str).unwrap_or("");
    let peer = if verb == "add" && args.len() == 5 && args[3] == "--peer" {
        Some(did_mini::Did::parse(&args[4]).map_err(|e| CliError::Identity(e.to_string()))?)
    } else {
        None
    };
    let expected = match verb {
        "list" => 1,
        "add" => if peer.is_some() { 5 } else { 3 },
        "remove" | "sync" => 2,
        _ => {
            return Err(usage(
                "remote add <name> <ip:port> [--peer <trusted-did>] | list | remove <name> | sync <name>",
            ))
        }
    };
    if args.len() != expected {
        return Err(usage("unexpected or missing remote arguments"));
    }
    // Lock only configuration access, not the potentially slow network exchange.
    mini_durable::create_dir_all(home).map_err(|e| CliError::Io(e.to_string()))?;
    let lock = mini_durable::lock_exclusive(&home.join("remotes.lock"))
        .map_err(|e| CliError::Io(e.to_string()))?;
    let mut remotes = load(home)?;
    let result = match verb {
        "list" => {
            let entries = remotes
                .iter()
                .map(|(name, addr)| {
                    JsonValue::Object(vec![
                        ("name".into(), JsonValue::str(name)),
                        ("address".into(), JsonValue::str(&addr.address)),
                        (
                            "peer".into(),
                            JsonValue::opt_str(addr.peer.as_ref().map(|p| p.as_str())),
                        ),
                        ("authenticated".into(), JsonValue::Bool(addr.peer.is_some())),
                    ])
                })
                .collect();
            CommandResult::new(
                remotes
                    .iter()
                    .map(|(name, addr)| {
                        format!(
                            "{name}\t{}\t{}",
                            addr.address,
                            addr.peer
                                .as_ref()
                                .map(|p| p.as_str())
                                .unwrap_or("legacy-unpinned")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .field("remotes", JsonValue::Array(entries))
        }
        "add" | "remove" => {
            let name = &args[1];
            if !valid_name(name) {
                return Err(usage("invalid remote name"));
            }
            if verb == "add" {
                let address: SocketAddr = args[2]
                    .parse()
                    .map_err(|_| usage("remote address must be an IP and port"))?;
                if address.port() == 0 || address.ip().is_unspecified() {
                    return Err(usage("remote address must name a peer and nonzero port"));
                }
                if remotes.contains_key(name) {
                    return Err(usage(
                        "remote already exists; remove it explicitly before replacing",
                    ));
                }
                if remotes.len() >= MAX_REMOTES {
                    return Err(usage("too many remotes"));
                }
                remotes.insert(
                    name.clone(),
                    Remote {
                        address: address.to_string(),
                        peer,
                    },
                );
            } else if remotes.remove(name).is_none() {
                return Err(usage("unknown remote"));
            }
            let text = remotes
                .iter()
                .map(|(name, addr)| match &addr.peer {
                    Some(peer) => format!("{name}\t{}\t{}\n", addr.address, peer.as_str()),
                    None => format!("{name}\t{}\n", addr.address),
                })
                .collect::<String>();
            mini_durable::atomic_replace(&home.join("remotes"), text.as_bytes())
                .map_err(|e| CliError::Io(e.to_string()))?;
            CommandResult::new(format!("remote {verb}: {name}")).field("name", JsonValue::str(name))
        }
        "sync" => {
            let remote = remotes
                .get(&args[1])
                .ok_or_else(|| usage("unknown remote"))?
                .clone();
            drop(lock);
            let report = match &remote.peer {
                Some(peer) => {
                    crate::sync::connect_authenticated_report(home, store, &remote.address, peer)?
                }
                None => crate::sync::connect_report(home, store, &remote.address)?,
            };
            return Ok(CommandResult::new(format!(
                "synced {}: received {}, accepted {} (local ingest; complete: {}; repeat sync if false)",
                args[1], report.received, report.accepted, report.complete
            ))
            .field("name", JsonValue::str(&args[1]))
            .field("address", JsonValue::str(&remote.address))
            .field(
                "peer",
                JsonValue::opt_str(remote.peer.as_ref().map(|p| p.as_str())),
            )
            .field("authenticated", JsonValue::Bool(remote.peer.is_some()))
            .field("complete", JsonValue::Bool(report.complete))
            .field("continue_required", JsonValue::Bool(!report.complete))
            .field("received", JsonValue::num(report.received as u64))
            .field("accepted", JsonValue::num(report.accepted as u64))
            .field(
                "unknown_author",
                JsonValue::num(report.unknown_author as u64),
            )
            .field("invalid", JsonValue::num(report.invalid as u64))
            .field("unsolicited", JsonValue::num(report.unsolicited as u64))
            .field("carriers", JsonValue::num(report.carriers as u64))
            .field("canonicalized", JsonValue::Bool(false))
            .render(json, "remote.sync"));
        }
        _ => unreachable!(),
    };
    drop(lock);
    Ok(result.render(json, &format!("remote.{verb}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn persisted_sorted_endpoints_and_explicit_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            dispatch(
                dir.path(),
                &dir.path().join("store"),
                args.iter().map(|s| s.to_string()).collect(),
                true,
            )
        };
        run(&["add", "origin", "127.0.0.1:39001"]).unwrap();
        assert!(run(&["add", "origin", "127.0.0.1:39002"]).is_err());
        assert!(run(&["list"]).unwrap().contains("127.0.0.1:39001"));
        run(&["remove", "origin"]).unwrap();
        assert!(run(&["list"]).unwrap().contains("\"remotes\":[]"));
        assert!(run(&["sync", "origin"]).is_err());
    }
    #[test]
    fn rejects_injection_and_corrupt_configuration() {
        let dir = tempfile::tempdir().unwrap();
        for args in [
            vec!["add", "../escape", "127.0.0.1:1"],
            vec!["add", "ok", "0.0.0.0:0"],
            vec!["list", "extra"],
        ] {
            assert!(dispatch(
                dir.path(),
                dir.path(),
                args.into_iter().map(String::from).collect(),
                true
            )
            .is_err());
        }
        fs::write(
            dir.path().join("remotes"),
            "origin\t127.0.0.1:1\norigin\t127.0.0.1:2\n",
        )
        .unwrap();
        assert!(load(dir.path()).is_err());
    }
}
