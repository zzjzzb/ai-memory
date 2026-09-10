//! Transparent performance: embed LRU, open() PRAGMAs, prune + TTL/isolation.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ai_memory::{
    Embedder, HashEmbedder, MemoryPolicy, MemoryStore, RecallQuery, RememberRequest, SqliteStore,
    Tier,
};

struct CountingEmbedder {
    inner: HashEmbedder,
    hits: AtomicU64,
}

impl CountingEmbedder {
    fn new() -> Self {
        Self {
            inner: HashEmbedder::new(32),
            hits: AtomicU64::new(0),
        }
    }
}

impl Embedder for CountingEmbedder {
    fn dim(&self) -> usize {
        self.inner.dim()
    }

    fn embed(&self, text: &str) -> ai_memory::Result<Vec<f32>> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        self.inner.embed(text)
    }
}

#[test]
fn embed_cache_skips_duplicate_text() {
    let counter = Arc::new(CountingEmbedder::new());
    let store = SqliteStore::builder()
        .in_memory()
        .embedder(Arc::clone(&counter) as Arc<dyn Embedder>)
        .build()
        .unwrap();
    store.create_project("a", MemoryPolicy::default()).unwrap();
    store.create_project("b", MemoryPolicy::default()).unwrap();

    let text = "shared cacheable sentence about linen";
    store.remember("a", RememberRequest::new(text)).unwrap();
    let after_first = counter.hits.load(Ordering::SeqCst);
    assert_eq!(after_first, 1);

    store.remember("b", RememberRequest::new(text)).unwrap();
    assert_eq!(
        counter.hits.load(Ordering::SeqCst),
        1,
        "identical text+dim must reuse the in-process embed cache across projects"
    );

    store
        .recall("a", RecallQuery::new(text).with_limit(3))
        .unwrap();
    assert_eq!(
        counter.hits.load(Ordering::SeqCst),
        1,
        "recall of the same text must hit the embed cache"
    );

    store
        .remember("a", RememberRequest::new("a different sentence entirely"))
        .unwrap();
    assert_eq!(counter.hits.load(Ordering::SeqCst), 2);
}

#[test]
fn open_in_memory_applies_core_pragmas() {
    let store = SqliteStore::open_in_memory().unwrap();
    let p = store.applied_pragmas().unwrap();
    assert!(p.foreign_keys);
    assert_eq!(p.temp_store, 2, "temp_store=MEMORY");
    assert_eq!(p.cache_size, -16384);
    assert_eq!(p.busy_timeout_ms, 5000);
}

#[test]
fn open_file_applies_wal_and_normal_sync() {
    let path = std::env::temp_dir().join(format!(
        "ai-memory-pragma-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    let store = SqliteStore::open(&path).unwrap();
    let p = store.applied_pragmas().unwrap();
    assert!(p.foreign_keys);
    assert_eq!(p.journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(p.synchronous, 1, "NORMAL");
    assert_eq!(p.temp_store, 2);
    assert_eq!(p.busy_timeout_ms, 5000);
    drop(store);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn recall_prune_still_respects_ttl_pin_and_isolation() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut policy = MemoryPolicy::default();
    policy.retention.working = Some(Duration::from_secs(0));
    policy.promote.pinned_skip_expiry = true;
    policy.recall.candidate_prune = 8;
    policy.recall.scan_limit = 64;
    store.create_project("desk", policy.clone()).unwrap();
    store
        .create_project("other", MemoryPolicy::default())
        .unwrap();

    let other = store
        .remember(
            "other",
            RememberRequest::new("other-project unique-isolation-token"),
        )
        .unwrap();

    let mut last_ephemeral = String::new();
    for i in 0..12 {
        let m = store
            .remember(
                "desk",
                RememberRequest::new(format!("ephemeral unique-isolation-token {i}"))
                    .with_tier(Tier::Working),
            )
            .unwrap();
        last_ephemeral = m.id;
    }
    // Live rows so candidate prune actually runs (working TTL=0 expires the loop above).
    for i in 0..20 {
        store
            .remember(
                "desk",
                RememberRequest::new(format!("episodic unique-isolation-token {i}"))
                    .with_tier(Tier::Episodic),
            )
            .unwrap();
    }
    let keep = store
        .remember(
            "desk",
            RememberRequest::new("pinned unique-isolation-token stays").with_tier(Tier::Working),
        )
        .unwrap();
    store.pin("desk", &keep.id).unwrap();

    let hits = store
        .recall(
            "desk",
            RecallQuery::new("unique-isolation-token").with_limit(16),
        )
        .unwrap();
    assert!(hits.iter().all(|h| h.memory.project_id == "desk"));
    assert!(hits.iter().all(|h| h.memory.id != other.id));
    assert!(hits.iter().all(|h| h.memory.id != last_ephemeral));
    assert!(
        hits.iter().any(|h| h.memory.id == keep.id),
        "pinned row must survive TTL + prune"
    );
}

#[test]
fn store_session_entrypoint() {
    let store = SqliteStore::open_in_memory().unwrap();
    store.create_project("bot", MemoryPolicy::chat()).unwrap();
    let session = store.session("bot").unwrap();
    assert_eq!(session.project_id(), "bot");
    session
        .remember(RememberRequest::new("hello from session"))
        .unwrap();
    assert!(!session.prefetch("hello").unwrap().is_empty());
}
