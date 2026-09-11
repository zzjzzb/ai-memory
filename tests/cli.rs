//! Smoke the `ai-memory` CLI against an isolated temp db (Rust is the store).

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ai-memory"))
}

fn temp_db() -> String {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ai-memory-cli-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path.to_string_lossy().into_owned()
}

fn stdout_json(output: std::process::Output) -> serde_json::Value {
    let text = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!(
            "cli json parse failed: {e}; stdout={text}; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn cli_remember_prefetch_forget() {
    let db = temp_db();
    let remember = cli()
        .args([
            "--db",
            &db,
            "--project",
            "cli-demo",
            "memory_remember",
            r#"{"text":"User prefers linen shirts","tier":"profile"}"#,
        ])
        .output()
        .expect("spawn remember");
    assert!(remember.status.success(), "{remember:?}");
    let saved = stdout_json(remember);
    assert_eq!(saved["ok"], true, "{saved}");
    let id = saved["data"]["id"].as_str().unwrap().to_string();

    let pack = cli()
        .args([
            "--db",
            &db,
            "--project",
            "cli-demo",
            "prefetch_within_budget",
            r#"{"query":"linen","max_tokens":256}"#,
        ])
        .output()
        .expect("spawn prefetch");
    assert!(pack.status.success());
    let pack = stdout_json(pack);
    assert_eq!(pack["ok"], true, "{pack}");
    assert!(pack["data"]["text"]
        .as_str()
        .unwrap()
        .contains("linen shirts"));

    let forgot = cli()
        .args([
            "--db",
            &db,
            "--project",
            "cli-demo",
            "memory_forget",
            &format!(r#"{{"memory_id":"{id}"}}"#),
        ])
        .output()
        .expect("spawn forget");
    assert!(forgot.status.success());
    let _ = std::fs::remove_file(&db);
}
