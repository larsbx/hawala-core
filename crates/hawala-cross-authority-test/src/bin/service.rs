use ed25519_dalek::SigningKey;
use hawala_cross_authority_test::endpoints::{unhex, Policy, Service};
use std::{collections::BTreeMap, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--synthetic-only") {
        return Err("TEST-only service requires --synthetic-only".into());
    }
    let mut opts = BTreeMap::new();
    let mut initialize = false;
    let mut drop_reply = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--initialize" => {
                initialize = true;
                i += 1;
            }
            "--drop-first-commit-reply" => {
                drop_reply = true;
                i += 1;
            }
            flag @ ("--loopback-port-file"
            | "--store"
            | "--socket"
            | "--host-public"
            | "--service-seed"
            | "--grant"
            | "--epoch"
            | "--max-requests") => {
                let value = args.get(i + 1).ok_or("missing value")?.clone();
                if opts.insert(flag.to_owned(), value).is_some() {
                    return Err("duplicate option".into());
                }
                i += 2;
            }
            _ => return Err("unknown TEST option".into()),
        }
    }
    let get = |k: &str| opts.get(k).ok_or("missing required TEST option");
    let service = Service::open(
        get("--store")?,
        Policy {
            host: unhex(get("--host-public")?)?,
            grant: unhex(get("--grant")?)?,
            epoch: get("--epoch")?.parse()?,
            allowed: 15,
        },
        SigningKey::from_bytes(&unhex(get("--service-seed")?)?),
        initialize,
    )?;
    let maximum = opts.get("--max-requests").map_or(Ok(0), |v| v.parse())?;
    match (opts.get("--loopback-port-file"), opts.get("--socket")) {
        (Some(path), None) => service.serve_loopback(Path::new(path), maximum, drop_reply),
        (None, Some(path)) => service.serve(Path::new(path), maximum, drop_reply),
        _ => Err("choose exactly one local TEST transport".into()),
    }
}
