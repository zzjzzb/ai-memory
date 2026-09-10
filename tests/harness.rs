//! Harness adapter contracts: session isolation, tool JSON, ContextPack, batch, TTL.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ai_memory::{
    memory_tool_specs, AgentSession, BruteForceCosine, Error, HashEmbedder, MemoryPolicy,
    MemoryStore, RecallQuery, RememberRequest, SqliteStore, Tier, VectorIndex, TOOL_CONSOLIDATE,
    TOOL_FORGET, TOOL_PIN, TOOL_RECALL, TOOL_REMEMBER,
};
use serde_json::json;

fn sqlite() -> SqliteStore {
    SqliteStore::builder()
        .in_memory()
        .embedder(Arc::new(HashEmbedder::new(32)))
        .build()
        .unwrap()
}

#[test]
fn harness_sessions_do_not_cross_projects() {
    let store = sqlite();
    store
        .create_project("agent-a", MemoryPolicy::chat())
        .unwrap();
    store
        .create_project("agent-b", MemoryPolicy::journal())
        .unwrap();
    let a = AgentSession::sqlite(store.clone(), "agent-a").unwrap();
    let b = AgentSession::sqlite(store, "agent-b").unwrap();

    a.call_tool(
        TOOL_REMEMBER,
        json!({"text": "alpha-only secret phrase", "tier": "profile"}),
    );
    b.call_tool(
        TOOL_REMEMBER,
        json!({"text": "beta-only mango phrase", "tier": "working"}),
    );

    let a_hits = a.prefetch("secret phrase").unwrap();
    assert!(a_hits.iter().all(|h| h.memory.project_id == "agent-a"));
    assert!(a_hits.iter().all(|h| !h.memory.text.contains("mango")));

    let b_hits = b.prefetch("alpha-only").unwrap();
    assert!(b_hits.iter().all(|h| !h.memory.text.contains("alpha-only")));
}

#[test]
fn tool_dispatch_remember_recall_roundtrip() {
    let store = sqlite();
    store.create_project("p", MemoryPolicy::default()).unwrap();
    let s = AgentSession::sqlite(store, "p").unwrap();

    let specs = s.tool_specs();
    assert_eq!(specs.len(), memory_tool_specs().len());
    assert!(specs.iter().any(|t| t.name == TOOL_REMEMBER));
    let openai = specs[0].openai_tool();
    assert_eq!(openai["type"], "function");
    let anth = specs[0].anthropic_tool();
    assert!(anth.get("input_schema").is_some());

    let saved = s.call_tool(
        TOOL_REMEMBER,
        json!({"text": "User prefers linen shirts", "tier": "profile"}),
    );
    assert!(saved.ok, "{saved:?}");
    let id = saved.data["id"].as_str().unwrap().to_string();

    let recalled = s.call_tool(TOOL_RECALL, json!({"text": "linen shirts", "limit": 4}));
    assert!(recalled.ok, "{recalled:?}");
    let hits = recalled.data["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0]["id"], id);
    let ctx = recalled.data["context"].as_str().unwrap();
    assert!(ctx.contains("linen shirts"));
    assert!(ctx.contains(&id));

    let pin = s.call_tool(TOOL_PIN, json!({"memory_id": id}));
    assert!(pin.ok);
    let cons = s.call_tool(TOOL_CONSOLIDATE, json!({}));
    assert!(cons.ok);
    let forgot = s.call_tool(TOOL_FORGET, json!({"memory_id": id}));
    assert!(forgot.ok);
    assert!(s.prefetch("linen").unwrap().is_empty());
}

#[test]
fn unknown_tool_is_safe_error() {
    let store = sqlite();
    store.create_project("p", MemoryPolicy::default()).unwrap();
    let s = AgentSession::sqlite(store, "p").unwrap();
    let r = s.call_tool("not_a_tool", json!({}));
    assert!(!r.ok);
    assert!(r.error.unwrap().contains("unknown tool"));
}

#[test]
fn context_pack_contains_expected_facts() {
    let store = sqlite();
    store
        .create_project("chat", MemoryPolicy::default())
        .unwrap();
    let s = AgentSession::sqlite(store, "chat").unwrap();
    s.remember_turn([
        RememberRequest::new("User prefers dark mode").with_tier(Tier::Profile),
        RememberRequest::new("scratch: sidebar layout").with_tier(Tier::Working),
    ])
    .unwrap();

    let hits = s.prefetch("dark mode preference").unwrap();
    let pack = s.pack_context(&hits);
    let rendered = pack.render();
    assert!(rendered.contains("project: chat"));
    assert!(rendered.contains("User prefers dark mode"));
    assert!(pack.blocks.iter().any(|b| b.tier == Tier::Profile));
    assert!(pack.blocks.iter().any(|b| rendered.contains(&b.id)));
}

#[test]
fn batch_remember_is_all_or_nothing() {
    let store = sqlite();
    store.create_project("p", MemoryPolicy::default()).unwrap();

    let written = store
        .remember_many(
            "p",
            vec![
                RememberRequest::new("first batch note"),
                RememberRequest::new("second batch note"),
            ],
        )
        .unwrap();
    assert_eq!(written.len(), 2);
    assert_eq!(store.list_memories("p").unwrap().len(), 2);

    let err = store
        .remember_many(
            "p",
            vec![
                RememberRequest::new("should not land"),
                RememberRequest::new("   "),
            ],
        )
        .unwrap_err();
    match err {
        Error::EmptyText => {}
        other => panic!("{other:?}"),
    }
    let texts: Vec<_> = store
        .list_memories("p")
        .unwrap()
        .into_iter()
        .map(|m| m.text)
        .collect();
    assert_eq!(texts.len(), 2);
    assert!(texts.iter().all(|t| t != "should not land"));
}

#[test]
fn prefetch_respects_ttl_and_pin() {
    let store = sqlite();
    let mut policy = MemoryPolicy::default();
    policy.retention.working = Some(Duration::from_secs(0));
    policy.promote.pinned_skip_expiry = true;
    store.create_project("desk", policy).unwrap();
    let s = AgentSession::sqlite(store, "desk").unwrap();

    let ephemeral = s
        .remember_turn([RememberRequest::new("ttl-canary-ephemeral").with_tier(Tier::Working)])
        .unwrap();
    let keep = s
        .remember_turn([RememberRequest::new("ttl-canary-pinned").with_tier(Tier::Working)])
        .unwrap();
    s.call_tool(TOOL_PIN, json!({"memory_id": keep[0].id}));

    let hits = s.prefetch("ttl-canary").unwrap();
    assert!(hits.iter().all(|h| h.memory.id != ephemeral[0].id));
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, keep[0].id);
}

#[test]
fn candidate_prune_limits_vector_index_batch() {
    struct CountingIndex {
        inner: BruteForceCosine,
        n: Arc<AtomicUsize>,
    }
    impl VectorIndex for CountingIndex {
        fn similar(
            &self,
            query: &[f32],
            candidates: &[(String, Vec<f32>)],
        ) -> ai_memory::Result<Vec<(String, f32)>> {
            self.n.store(candidates.len(), Ordering::SeqCst);
            self.inner.similar(query, candidates)
        }
    }

    let n = Arc::new(AtomicUsize::new(0));
    let store = SqliteStore::builder()
        .in_memory()
        .vector_index(Arc::new(CountingIndex {
            inner: BruteForceCosine,
            n: Arc::clone(&n),
        }))
        .build()
        .unwrap();

    let mut policy = MemoryPolicy::default();
    policy.recall.candidate_prune = 5;
    store.create_project("p", policy).unwrap();
    for i in 0..20 {
        store
            .remember("p", RememberRequest::new(format!("noise filler {i} lorem")))
            .unwrap();
    }
    store
        .remember("p", RememberRequest::new("unique-prune-token zebra"))
        .unwrap();

    let hits = store
        .recall("p", RecallQuery::new("unique-prune-token").with_limit(3))
        .unwrap();
    assert_eq!(n.load(Ordering::SeqCst), 5);
    assert!(hits
        .iter()
        .any(|h| h.memory.text.contains("unique-prune-token")));
}

#[test]
fn session_missing_project_errors() {
    let store = sqlite();
    match AgentSession::sqlite(store, "nope") {
        Err(Error::ProjectNotFound(id)) => assert_eq!(id, "nope"),
        other => panic!("{other:?}"),
    }
}
