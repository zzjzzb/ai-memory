//! SME support / ops scenario smoke (same seed as `scenarios/dsh-support-agent`).
//!
//! Seed → `memory_remember` / `memory_pin` → `prefetch_within_budget`.
//! Asserts pack ≤ budget, pins survive a tight budget, and `sme-hr` does not
//! leak the sidebar ticket. Rust `HostSession` is the store (no JS memory).

use std::collections::HashMap;

use ai_memory::host::{HostSession, OP_PREFETCH};
use serde_json::{json, Value};

const SEED_JSON: &str = include_str!("../scenarios/dsh-support-agent/seed/tickets.json");

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Seed {
    projects: HashMap<String, ProjectSeed>,
    token_budget: usize,
    tight_budget: usize,
    pin_marker: String,
    tickets: Vec<Ticket>,
    isolation_notes: Vec<Note>,
    inflate: Inflate,
    observe: Observe,
}

#[derive(Debug, serde::Deserialize)]
struct ProjectSeed {
    policy: String,
}

#[derive(Debug, serde::Deserialize)]
struct Ticket {
    project: String,
    turns: Vec<Note>,
    #[serde(default)]
    facts: Vec<Note>,
}

#[derive(Debug, serde::Deserialize)]
struct Note {
    text: String,
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    pin: bool,
}

#[derive(Debug, serde::Deserialize)]
struct Inflate {
    count: usize,
    text: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Observe {
    sidebar_query: String,
    billing_query: String,
    isolation_query: String,
}

fn seed() -> Seed {
    serde_json::from_str(SEED_JSON).expect("tickets.json")
}

fn chars_per_4(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

fn require_ok(v: &Value, label: &str) -> &Value {
    assert_eq!(v["ok"], true, "{label}: {v}");
    v
}

fn remember(host: &HostSession, note: &Note) -> String {
    let mut args = json!({ "text": note.text });
    if let Some(tier) = &note.tier {
        args["tier"] = json!(tier);
    }
    let saved = require_ok(&host.dispatch("memory_remember", args), "memory_remember").clone();
    let id = saved["data"]["id"].as_str().expect("id").to_string();
    if note.pin {
        require_ok(
            &host.dispatch("memory_pin", json!({ "memory_id": id })),
            "memory_pin",
        );
    }
    id
}

fn prefetch(host: &HostSession, query: &str, max_tokens: usize) -> Value {
    require_ok(
        &host.dispatch(
            OP_PREFETCH,
            json!({ "query": query, "max_tokens": max_tokens }),
        ),
        "prefetch_within_budget",
    )
    .clone()
}

fn temp_db() -> String {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dsh-support-scenario-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path.to_string_lossy().into_owned()
}

fn transcript_tokens(seed: &Seed) -> usize {
    let mut buf = String::new();
    for ticket in &seed.tickets {
        for turn in &ticket.turns {
            buf.push_str(&turn.text);
            buf.push('\n');
        }
        for fact in &ticket.facts {
            buf.push_str(&fact.text);
            buf.push('\n');
        }
    }
    for n in 1..=seed.inflate.count {
        buf.push_str(&seed.inflate.text.replace("{n}", &n.to_string()));
        buf.push('\n');
    }
    chars_per_4(&buf)
}

#[test]
fn sme_support_seed_remember_budgeted_prefetch_pins_and_isolation() {
    let seed = seed();
    let db = temp_db();
    let support_policy = &seed.projects["sme-support"].policy;
    let hr_policy = &seed.projects["sme-hr"].policy;
    let support = HostSession::open(&db, "sme-support", support_policy).unwrap();
    let hr = HostSession::open(&db, "sme-hr", hr_policy).unwrap();

    let mut remembered = 0usize;
    for ticket in &seed.tickets {
        assert_eq!(ticket.project, "sme-support");
        for turn in &ticket.turns {
            remember(&support, turn);
            remembered += 1;
        }
        for fact in &ticket.facts {
            remember(&support, fact);
            remembered += 1;
        }
    }
    for n in 1..=seed.inflate.count {
        remember(
            &support,
            &Note {
                text: seed.inflate.text.replace("{n}", &n.to_string()),
                tier: Some("working".into()),
                pin: false,
            },
        );
        remembered += 1;
    }
    for note in &seed.isolation_notes {
        remember(&hr, note);
        remembered += 1;
    }
    assert!(remembered > seed.inflate.count);

    let sidebar = prefetch(&support, &seed.observe.sidebar_query, seed.token_budget);
    let tight = prefetch(&support, &seed.observe.billing_query, seed.tight_budget);
    let isolated = prefetch(&hr, &seed.observe.isolation_query, seed.token_budget);

    let sidebar_tokens = sidebar["data"]["tokens"].as_u64().unwrap() as usize;
    let tight_tokens = tight["data"]["tokens"].as_u64().unwrap() as usize;
    let sidebar_text = sidebar["data"]["text"].as_str().unwrap();
    let tight_text = tight["data"]["text"].as_str().unwrap();
    let hr_text = isolated["data"]["text"].as_str().unwrap();

    assert!(
        sidebar_tokens <= seed.token_budget,
        "sidebar pack {sidebar_tokens} > {}",
        seed.token_budget
    );
    assert!(
        tight_tokens <= seed.tight_budget,
        "tight pack {tight_tokens} > {}",
        seed.tight_budget
    );
    assert!(
        transcript_tokens(&seed) > seed.token_budget,
        "raw transcript should exceed the pack budget"
    );
    assert!(
        tight_text.contains(&seed.pin_marker),
        "pin must survive tight budget: {tight_text}"
    );
    assert!(
        sidebar_text.contains("T-1042") || sidebar_text.to_lowercase().contains("sidebar"),
        "sidebar pack should cite T-1042: {sidebar_text}"
    );
    assert!(
        !hr_text.contains("T-1042") && !hr_text.to_lowercase().contains("sidebar overlap"),
        "sme-hr leaked support ticket: {hr_text}"
    );
    assert!(
        hr_text.to_lowercase().contains("handbook") || hr_text.contains("HR only"),
        "sme-hr pack should keep isolation notes: {hr_text}"
    );
    assert!(sidebar_text.contains("## Memory (project: sme-support"));
}
