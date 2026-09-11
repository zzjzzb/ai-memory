//! JSON CLI for the DeepSeek Harness plugin (and humans).
//!
//! Memory logic stays in the `ai-memory` library. This binary only parses argv
//! and prints a `{ok, name, data, error}` envelope.
//!
//! ```bash
//! ai-memory --db ./memory.db --project dsh memory_remember '{"text":"hi","tier":"profile"}'
//! ai-memory --in-memory --project demo prefetch_within_budget '{"query":"hi","max_tokens":512}'
//! ```

use std::env;
use std::process::ExitCode;

use ai_memory::host::HostSession;
use serde_json::{json, Value};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(value) => {
            println!("{value}");
            if value["ok"] == false {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(err) => {
            println!(
                "{}",
                json!({
                    "ok": false,
                    "name": "ai-memory",
                    "data": null,
                    "error": err,
                })
            );
            ExitCode::from(1)
        }
    }
}

fn run(args: Vec<String>) -> Result<Value, String> {
    let parsed = parse_args(&args)?;
    let host = if parsed.in_memory {
        HostSession::open_in_memory(&parsed.project, &parsed.policy)
    } else {
        HostSession::open(&parsed.db, &parsed.project, &parsed.policy)
    }
    .map_err(|e| e.to_string())?;
    Ok(host.dispatch(&parsed.op, parsed.payload))
}

struct Parsed {
    db: String,
    project: String,
    policy: String,
    in_memory: bool,
    op: String,
    payload: Value,
}

fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut db = String::from("./memory.db");
    let mut project = String::from("dsh");
    let mut policy = String::from("chat");
    let mut in_memory = false;
    let mut rest: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| "missing value for --db".to_string())?;
            }
            "--project" => {
                i += 1;
                project = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| "missing value for --project".to_string())?;
            }
            "--policy" => {
                i += 1;
                policy = args
                    .get(i)
                    .cloned()
                    .ok_or_else(|| "missing value for --policy".to_string())?;
            }
            "--in-memory" => in_memory = true,
            "--help" | "-h" => return Err(help().into()),
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag `{flag}`"));
            }
            other => rest.push(other),
        }
        i += 1;
    }
    let op = rest
        .first()
        .copied()
        .ok_or_else(|| help().to_string())?
        .to_string();
    let payload = match rest.get(1) {
        None => json!({}),
        Some(raw) => serde_json::from_str(raw).map_err(|e| format!("invalid JSON args: {e}"))?,
    };
    Ok(Parsed {
        db,
        project,
        policy,
        in_memory,
        op,
        payload,
    })
}

fn help() -> &'static str {
    "usage: ai-memory [--db PATH] [--project ID] [--policy chat|journal|default] [--in-memory] <op> [json-args]\n\
     ops: memory_remember | memory_recall | memory_forget | memory_pin | memory_consolidate | memory_compact | prefetch_within_budget"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_remember_json() {
        let parsed = parse_args(&[
            "--in-memory".into(),
            "--project".into(),
            "demo".into(),
            "memory_remember".into(),
            r#"{"text":"hello"}"#.into(),
        ])
        .unwrap();
        assert!(parsed.in_memory);
        assert_eq!(parsed.project, "demo");
        assert_eq!(parsed.op, "memory_remember");
        assert_eq!(parsed.payload["text"], "hello");
    }
}
