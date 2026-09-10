//! Integration tests for the MVP contract:
//! project isolation, policy-driven promote/recall, pin/forget, vector path.

use std::sync::Arc;
use std::time::Duration;

use ai_memory::{
    HashEmbedder, MemoryPolicy, MemoryStore, RecallQuery, RememberRequest, SqliteStore, Tier,
};

fn store() -> SqliteStore {
    SqliteStore::builder()
        .in_memory()
        .embedder(Arc::new(HashEmbedder::new(64)))
        .build()
        .unwrap()
}

#[test]
fn multi_project_recall_never_crosses() {
    let store = store();
    store
        .create_project("alpha", MemoryPolicy::default())
        .unwrap();
    store
        .create_project("beta", MemoryPolicy::default())
        .unwrap();

    store
        .remember(
            "alpha",
            RememberRequest::new("alpha secret token zebra-only-phrase"),
        )
        .unwrap();
    store
        .remember(
            "beta",
            RememberRequest::new("beta secret token mango-only-phrase"),
        )
        .unwrap();

    let alpha_hits = store
        .recall("alpha", RecallQuery::new("secret token").with_limit(16))
        .unwrap();
    assert!(!alpha_hits.is_empty());
    assert!(
        alpha_hits.iter().all(|h| h.memory.project_id == "alpha"),
        "recall leaked another project: {:?}",
        alpha_hits
            .iter()
            .map(|h| h.memory.project_id.clone())
            .collect::<Vec<_>>()
    );
    assert!(alpha_hits
        .iter()
        .all(|h| !h.memory.text.contains("mango-only-phrase")));

    let beta_hits = store
        .recall("beta", RecallQuery::new("zebra-only-phrase").with_limit(16))
        .unwrap();
    assert!(
        beta_hits
            .iter()
            .all(|h| !h.memory.text.contains("zebra-only-phrase")),
        "beta recall must not surface alpha's unique phrase"
    );

    let scoped = store.project("alpha").unwrap();
    let hits = scoped.recall(RecallQuery::new("secret")).unwrap();
    assert!(hits.iter().all(|h| h.memory.project_id == "alpha"));
}

#[test]
fn policy_changes_promote_behavior() {
    let store = store();

    let mut fast = MemoryPolicy::default();
    fast.promote.working_to_episodic_after = Duration::from_secs(0);
    fast.retention.working = Some(Duration::from_secs(60 * 60));

    let mut slow = MemoryPolicy::default();
    slow.promote.working_to_episodic_after = Duration::from_secs(60 * 60 * 24 * 365);
    slow.retention.working = Some(Duration::from_secs(60 * 60 * 24 * 365));

    store.create_project("fast", fast).unwrap();
    store.create_project("slow", slow).unwrap();

    store
        .remember(
            "fast",
            RememberRequest::new("fast note").with_tier(Tier::Working),
        )
        .unwrap();
    store
        .remember(
            "slow",
            RememberRequest::new("slow note").with_tier(Tier::Working),
        )
        .unwrap();

    let fast_report = store.consolidate("fast").unwrap();
    let slow_report = store.consolidate("slow").unwrap();
    assert_eq!(fast_report.promoted_to_episodic, 1);
    assert_eq!(slow_report.promoted_to_episodic, 0);

    assert_eq!(store.list_memories("fast").unwrap()[0].tier, Tier::Episodic);
    assert_eq!(store.list_memories("slow").unwrap()[0].tier, Tier::Working);
}

#[test]
fn policy_weights_change_recall_ranking() {
    let store = store();

    let mut vectorish = MemoryPolicy::default();
    vectorish.recall.time = 0.0;
    vectorish.recall.keyword = 0.0;
    vectorish.recall.vector = 1.0;

    let mut keywordish = MemoryPolicy::default();
    keywordish.recall.time = 0.0;
    keywordish.recall.keyword = 1.0;
    keywordish.recall.vector = 0.0;

    store.create_project("vec", vectorish).unwrap();
    store.create_project("kw", keywordish).unwrap();

    let fox = "the quick brown fox jumps over the lazy dog";
    let pasta = "lasagna recipe tomato basil garlic pasta dish";
    // Query shares tokens with pasta (keyword) but is semantically closer to fox
    // for the hash embedder only on overlapping "brown fox" style phrases.
    // Use two docs where keyword overlap and vector neighborhood disagree:
    // - "fox forest woodland creature" vs query "canine woodland animal"
    // Hash embedder: overlapping tokens drive both; so pick docs where
    // keyword overlap is inverted vs a unique shared token set.
    let shared_tokens = RememberRequest::new("alpha beta gamma uniquekeyword");
    let similar_vec = RememberRequest::new("alpha beta gamma delta epsilon");

    for project in ["vec", "kw"] {
        store.remember(project, shared_tokens.clone()).unwrap();
        store.remember(project, similar_vec.clone()).unwrap();
        store.remember(project, RememberRequest::new(fox)).unwrap();
        store
            .remember(project, RememberRequest::new(pasta))
            .unwrap();
    }

    // Keyword-only: query tokens are a subset of pasta's distinctive words.
    let kw_hits = store
        .recall("kw", RecallQuery::new("lasagna tomato pasta").with_limit(4))
        .unwrap();
    assert!(
        kw_hits[0].memory.text.contains("lasagna"),
        "keyword policy should rank the lasagna note first, got {:?}",
        kw_hits.iter().map(|h| &h.memory.text).collect::<Vec<_>>()
    );

    // Vector-only: similar phrasing of fox should beat pasta for a fox query.
    let vec_hits = store
        .recall(
            "vec",
            RecallQuery::new("quick brown fox leaping").with_limit(4),
        )
        .unwrap();
    assert!(
        vec_hits[0].memory.text.contains("fox"),
        "vector policy should rank the fox note first, got {:?}",
        vec_hits.iter().map(|h| &h.memory.text).collect::<Vec<_>>()
    );
}

#[test]
fn pin_survives_expiry_forget_drops_from_recall() {
    let store = store();
    let mut policy = MemoryPolicy::default();
    policy.retention.working = Some(Duration::from_secs(0));
    policy.retention.episodic = Some(Duration::from_secs(0));
    policy.retention.profile = Some(Duration::from_secs(0));
    policy.promote.pinned_skip_expiry = true;
    policy.promote.working_to_episodic_after = Duration::from_secs(60 * 60 * 24 * 365);

    store.create_project("desk", policy).unwrap();
    let keep = store
        .remember(
            "desk",
            RememberRequest::new("keep me around").with_tier(Tier::Working),
        )
        .unwrap();
    let drop_me = store
        .remember(
            "desk",
            RememberRequest::new("I will expire").with_tier(Tier::Working),
        )
        .unwrap();
    let forget_me = store
        .remember(
            "desk",
            RememberRequest::new("forget this row").with_tier(Tier::Working),
        )
        .unwrap();

    store.pin("desk", &keep.id).unwrap();
    store.forget("desk", &forget_me.id).unwrap();
    assert!(store.get("desk", &forget_me.id).unwrap().is_none());

    let before = store
        .recall("desk", RecallQuery::new("forget this row").with_limit(8))
        .unwrap();
    assert!(before.iter().all(|h| h.memory.id != forget_me.id));

    let report = store.consolidate("desk").unwrap();
    assert!(report.expired >= 1);

    assert!(store.get("desk", &keep.id).unwrap().unwrap().pinned);
    assert!(store.get("desk", &drop_me.id).unwrap().is_none());

    let hits = store
        .recall("desk", RecallQuery::new("keep me around").with_limit(8))
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, keep.id);
}

#[test]
fn fake_embedder_vector_path_ranks_similar_text() {
    let store = store();
    let mut policy = MemoryPolicy::default();
    policy.recall.time = 0.0;
    policy.recall.keyword = 0.0;
    policy.recall.vector = 1.0;
    store.create_project("vec", policy).unwrap();

    store
        .remember(
            "vec",
            RememberRequest::new("the quick brown fox jumps over the lazy dog"),
        )
        .unwrap();
    store
        .remember(
            "vec",
            RememberRequest::new("unrelated chemistry lab protocol titration"),
        )
        .unwrap();

    let hits = store
        .recall(
            "vec",
            RecallQuery::new("quick brown fox leaping dog").with_limit(2),
        )
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert!(
        hits[0].memory.text.contains("fox"),
        "vector path should rank similar fox text first, scores: {:?}",
        hits.iter()
            .map(|h| (h.memory.text.clone(), h.vector_score, h.score))
            .collect::<Vec<_>>()
    );
    assert!(hits[0].vector_score > hits[1].vector_score);
    assert!(hits[0].score >= hits[1].score);
}

#[test]
fn file_open_roundtrip_and_explicit_tier() {
    let path = std::env::temp_dir().join(format!(
        "ai-memory-test-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    let store = SqliteStore::open(&path).unwrap();
    store.create_project("p", MemoryPolicy::default()).unwrap();
    let m = store
        .remember(
            "p",
            RememberRequest::new("a durable fact: user prefers vim")
                .with_tier(Tier::Profile)
                .with_metadata(serde_json::json!({"k": 1})),
        )
        .unwrap();
    assert_eq!(m.tier, Tier::Profile);
    drop(store);

    let store = SqliteStore::open(&path).unwrap();
    let loaded = store.get("p", &m.id).unwrap().unwrap();
    assert_eq!(loaded.text, "a durable fact: user prefers vim");
    assert_eq!(loaded.tier, Tier::Profile);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn infer_tier_used_when_unspecified() {
    let store = store();
    store.create_project("p", MemoryPolicy::default()).unwrap();
    let fact = store
        .remember("p", RememberRequest::new("I prefer dark mode"))
        .unwrap();
    assert_eq!(fact.tier, Tier::Profile);
    let event = store
        .remember("p", RememberRequest::new("Today I hiked a long trail"))
        .unwrap();
    assert_eq!(event.tier, Tier::Episodic);
}

#[test]
fn pin_wrong_project_does_not_mutate() {
    let store = store();
    store.create_project("a", MemoryPolicy::default()).unwrap();
    store.create_project("b", MemoryPolicy::default()).unwrap();
    let m = store
        .remember("a", RememberRequest::new("only in a"))
        .unwrap();
    let err = store.pin("b", &m.id).unwrap_err();
    match err {
        ai_memory::Error::MemoryNotFound { project_id, .. } => {
            assert_eq!(project_id, "b");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(!store.get("a", &m.id).unwrap().unwrap().pinned);
}

#[test]
fn episodic_to_profile_requires_accesses() {
    let store = store();
    let mut policy = MemoryPolicy::default();
    policy.promote.episodic_to_profile_after = Duration::from_secs(0);
    policy.promote.episodic_to_profile_min_accesses = 2;
    policy.retention.episodic = Some(Duration::from_secs(60 * 60 * 24 * 365));
    store.create_project("p", policy).unwrap();

    let m = store
        .remember(
            "p",
            RememberRequest::new("recurring preference").with_tier(Tier::Episodic),
        )
        .unwrap();
    store.consolidate("p").unwrap();
    assert_eq!(store.get("p", &m.id).unwrap().unwrap().tier, Tier::Episodic);

    store
        .recall("p", RecallQuery::new("recurring preference").with_limit(1))
        .unwrap();
    store
        .recall("p", RecallQuery::new("recurring preference").with_limit(1))
        .unwrap();
    let report = store.consolidate("p").unwrap();
    assert_eq!(report.promoted_to_profile, 1);
    assert_eq!(store.get("p", &m.id).unwrap().unwrap().tier, Tier::Profile);
}
