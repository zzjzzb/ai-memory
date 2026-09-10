//! Phase-2 SDK contracts (written as the spec, then implemented).
//! Project CRUD, list filters, TTL on recall, embedder/VectorIndex injection.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use ai_memory::{
    cosine, BruteForceCosine, Embedder, Error, HashEmbedder, MemoryListFilter, MemoryPolicy,
    MemoryStore, RecallQuery, RememberRequest, SqliteStore, Tier, VectorIndex,
};

fn store() -> SqliteStore {
    SqliteStore::builder()
        .in_memory()
        .embedder(Arc::new(HashEmbedder::new(64)))
        .build()
        .unwrap()
}

#[test]
fn project_crud_create_get_list_update_delete() {
    let store = store();
    assert!(store.list_projects().unwrap().is_empty());

    let mut policy = MemoryPolicy::default();
    policy.recall.vector = 0.9;
    policy.recall.keyword = 0.05;
    policy.recall.time = 0.05;
    store.create_project("alpha", policy.clone()).unwrap();
    store
        .create_project("beta", MemoryPolicy::default())
        .unwrap();

    let got = store.get_project("alpha").unwrap().unwrap();
    assert_eq!(got.id, "alpha");
    assert!((got.policy.recall.vector - 0.9).abs() < 1e-5);

    let ids: Vec<_> = store
        .list_projects()
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);

    let mut tighter = policy.clone();
    tighter.recall.vector = 0.1;
    tighter.recall.keyword = 0.8;
    tighter.recall.time = 0.1;
    store.set_policy("alpha", tighter).unwrap();
    let updated = store.policy("alpha").unwrap();
    assert!((updated.recall.keyword - 0.8).abs() < 1e-5);

    store
        .remember("alpha", RememberRequest::new("alpha-only-fact"))
        .unwrap();
    store
        .remember("beta", RememberRequest::new("beta-only-fact"))
        .unwrap();
    store.delete_project("alpha").unwrap();

    assert!(store.get_project("alpha").unwrap().is_none());
    match store.recall("alpha", RecallQuery::new("alpha-only-fact")) {
        Err(Error::ProjectNotFound(id)) => assert_eq!(id, "alpha"),
        other => panic!("expected ProjectNotFound, got {other:?}"),
    }
    let beta = store
        .recall("beta", RecallQuery::new("beta-only-fact").with_limit(4))
        .unwrap();
    assert_eq!(beta.len(), 1);
    assert!(beta[0].memory.text.contains("beta-only"));
}

#[test]
fn delete_missing_project_errors() {
    let store = store();
    match store.delete_project("ghost") {
        Err(Error::ProjectNotFound(id)) => assert_eq!(id, "ghost"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn get_and_list_are_project_scoped_with_filters() {
    let store = store();
    store.create_project("a", MemoryPolicy::default()).unwrap();
    store.create_project("b", MemoryPolicy::default()).unwrap();

    let working = store
        .remember(
            "a",
            RememberRequest::new("scratch note in a").with_tier(Tier::Working),
        )
        .unwrap();
    let profile = store
        .remember(
            "a",
            RememberRequest::new("I prefer linen shirts").with_tier(Tier::Profile),
        )
        .unwrap();
    let other = store
        .remember("b", RememberRequest::new("secret in b"))
        .unwrap();

    assert!(store.get("a", &other.id).unwrap().is_none());
    assert_eq!(
        store.get("a", &profile.id).unwrap().unwrap().text,
        "I prefer linen shirts"
    );

    let profiles = store
        .list_memories_filtered("a", MemoryListFilter::new().with_tiers(vec![Tier::Profile]))
        .unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].id, profile.id);

    let future = SystemTime::now() + Duration::from_secs(3600);
    let none = store
        .list_memories_filtered("a", MemoryListFilter::new().with_since(future))
        .unwrap();
    assert!(none.is_empty());

    let past = SystemTime::now() - Duration::from_secs(3600);
    let all = store
        .list_memories_filtered("a", MemoryListFilter::new().with_since(past))
        .unwrap();
    assert_eq!(all.len(), 2);

    let pinned_only = store
        .list_memories_filtered("a", MemoryListFilter::new().pinned_only())
        .unwrap();
    assert!(pinned_only.is_empty());
    store.pin("a", &working.id).unwrap();
    let pinned_only = store
        .list_memories_filtered("a", MemoryListFilter::new().pinned_only())
        .unwrap();
    assert_eq!(pinned_only.len(), 1);
    assert_eq!(pinned_only[0].id, working.id);
}

#[test]
fn ttl_hides_expired_unpinned_from_recall_without_consolidate() {
    let store = store();
    let mut policy = MemoryPolicy::default();
    policy.retention.working = Some(Duration::from_secs(0));
    policy.promote.pinned_skip_expiry = true;
    store.create_project("desk", policy).unwrap();

    let ephemeral = store
        .remember(
            "desk",
            RememberRequest::new("fleeting scratch unique-ttl-token").with_tier(Tier::Working),
        )
        .unwrap();
    let keep = store
        .remember(
            "desk",
            RememberRequest::new("pinned unique-ttl-token stays").with_tier(Tier::Working),
        )
        .unwrap();
    store.pin("desk", &keep.id).unwrap();

    let hits = store
        .recall("desk", RecallQuery::new("unique-ttl-token").with_limit(8))
        .unwrap();
    assert!(
        hits.iter().all(|h| h.memory.id != ephemeral.id),
        "expired unpinned memory leaked into recall: {:?}",
        hits.iter().map(|h| &h.memory.text).collect::<Vec<_>>()
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, keep.id);

    let listed = store.list_memories("desk").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, keep.id);

    let raw = store
        .list_memories_filtered("desk", MemoryListFilter::new().including_expired())
        .unwrap();
    assert_eq!(raw.len(), 2);
}

#[test]
fn updating_policy_ttl_changes_recall() {
    let store = store();
    store.create_project("p", MemoryPolicy::default()).unwrap();
    store
        .remember(
            "p",
            RememberRequest::new("policy-ttl-canary note").with_tier(Tier::Working),
        )
        .unwrap();
    assert_eq!(
        store
            .recall("p", RecallQuery::new("policy-ttl-canary").with_limit(4))
            .unwrap()
            .len(),
        1
    );

    let mut expired = MemoryPolicy::default();
    expired.retention.working = Some(Duration::from_secs(0));
    expired.promote.pinned_skip_expiry = true;
    store.set_policy("p", expired).unwrap();

    let hits = store
        .recall("p", RecallQuery::new("policy-ttl-canary").with_limit(4))
        .unwrap();
    assert!(
        hits.is_empty(),
        "after TTL=0 policy update, unpinned working memory should not recall"
    );
}

#[test]
fn embedder_is_injected_on_open() {
    struct MarkerEmbedder;

    impl Embedder for MarkerEmbedder {
        fn dim(&self) -> usize {
            8
        }

        fn embed(&self, text: &str) -> ai_memory::Result<Vec<f32>> {
            let mut v = vec![0.0f32; 8];
            if text.contains("MARKER_ALPHA") {
                v[0] = 1.0;
            } else if text.contains("MARKER_BETA") {
                v[1] = 1.0;
            } else {
                v[7] = 1.0;
            }
            Ok(v)
        }
    }

    let mut policy = MemoryPolicy::default();
    policy.recall.time = 0.0;
    policy.recall.keyword = 0.0;
    policy.recall.vector = 1.0;

    let store = ai_memory::open_in_memory_with_embedder(Arc::new(MarkerEmbedder)).unwrap();
    assert_eq!(store.embedder().dim(), 8);
    store.create_project("p", policy).unwrap();
    store
        .remember("p", RememberRequest::new("doc MARKER_ALPHA apples"))
        .unwrap();
    store
        .remember("p", RememberRequest::new("doc MARKER_BETA oranges"))
        .unwrap();

    let hits = store
        .recall("p", RecallQuery::new("query MARKER_ALPHA").with_limit(2))
        .unwrap();
    assert!(
        hits[0].memory.text.contains("MARKER_ALPHA"),
        "injected embedder should make MARKER_ALPHA nearest, got {:?}",
        hits.iter().map(|h| &h.memory.text).collect::<Vec<_>>()
    );
    assert!(hits[0].vector_score > hits[1].vector_score);
}

#[test]
fn custom_vector_index_is_used_on_recall() {
    struct CountingIndex {
        inner: BruteForceCosine,
        calls: Arc<AtomicU64>,
    }

    impl VectorIndex for CountingIndex {
        fn similar(
            &self,
            query: &[f32],
            candidates: &[(String, Vec<f32>)],
        ) -> ai_memory::Result<Vec<(String, f32)>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.similar(query, candidates)
        }
    }

    let calls = Arc::new(AtomicU64::new(0));
    let store = SqliteStore::builder()
        .in_memory()
        .embedder(Arc::new(HashEmbedder::new(32)))
        .vector_index(Arc::new(CountingIndex {
            inner: BruteForceCosine,
            calls: Arc::clone(&calls),
        }))
        .build()
        .unwrap();
    store.create_project("p", MemoryPolicy::default()).unwrap();
    store
        .remember("p", RememberRequest::new("vector index path"))
        .unwrap();
    store
        .recall("p", RecallQuery::new("vector index").with_limit(4))
        .unwrap();
    assert!(
        calls.load(Ordering::SeqCst) >= 1,
        "VectorIndex::similar must run on the recall path"
    );
}

#[test]
fn vector_index_scores_drive_ranking() {
    struct PreferLast;

    impl VectorIndex for PreferLast {
        fn similar(
            &self,
            _query: &[f32],
            candidates: &[(String, Vec<f32>)],
        ) -> ai_memory::Result<Vec<(String, f32)>> {
            let n = candidates.len();
            Ok(candidates
                .iter()
                .enumerate()
                .map(|(i, (id, _))| {
                    let s = if n > 0 && i == n - 1 { 1.0 } else { 0.0 };
                    (id.clone(), s)
                })
                .collect())
        }
    }

    let mut policy = MemoryPolicy::default();
    policy.recall.time = 0.0;
    policy.recall.keyword = 0.0;
    policy.recall.vector = 1.0;

    let store = SqliteStore::builder()
        .in_memory()
        .vector_index(Arc::new(PreferLast))
        .build()
        .unwrap();
    store.create_project("p", policy).unwrap();
    store
        .remember("p", RememberRequest::new("first zebra-token"))
        .unwrap();
    let second = store
        .remember("p", RememberRequest::new("second mango-token"))
        .unwrap();

    let hits = store
        .recall("p", RecallQuery::new("zebra-token").with_limit(2))
        .unwrap();
    assert_eq!(
        hits[0].memory.id, second.id,
        "custom VectorIndex should force the last inserted row to rank first"
    );
}

#[test]
fn consumer_two_project_flow() {
    let store = store();
    let mut chat = MemoryPolicy::chat();
    chat.promote.working_to_episodic_after = Duration::from_secs(0);
    let mut journal = MemoryPolicy::journal();
    journal.promote.working_to_episodic_after = Duration::from_secs(60 * 60 * 24 * 365);

    store.create_project("chat", chat).unwrap();
    store.create_project("journal", journal).unwrap();

    store
        .remember(
            "chat",
            RememberRequest::new("User prefers dark mode").with_tier(Tier::Profile),
        )
        .unwrap();
    store
        .remember(
            "journal",
            RememberRequest::new("Today I hiked Mount Tam").with_tier(Tier::Working),
        )
        .unwrap();

    let chat_hits = store
        .recall("chat", RecallQuery::new("theme preference").with_limit(5))
        .unwrap();
    assert!(chat_hits.iter().all(|h| h.memory.project_id == "chat"));
    assert!(chat_hits
        .iter()
        .all(|h| !h.memory.text.contains("Mount Tam")));

    store.consolidate("chat").unwrap();
    store.consolidate("journal").unwrap();
    assert_eq!(store.list_memories("chat").unwrap()[0].tier, Tier::Profile);
    assert_eq!(
        store.list_memories("journal").unwrap()[0].tier,
        Tier::Working
    );
}

#[test]
fn brute_force_cosine_matches_trait() {
    let idx = BruteForceCosine;
    let q = vec![1.0, 0.0];
    let cands = vec![("a".into(), vec![1.0, 0.0]), ("b".into(), vec![0.0, 1.0])];
    let out = idx.similar(&q, &cands).unwrap();
    assert!((out[0].1 - 1.0).abs() < 1e-5);
    assert!(out[1].1.abs() < 1e-5);
    assert!((cosine(&q, &cands[0].1) - 1.0).abs() < 1e-5);
}
