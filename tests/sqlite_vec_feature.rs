#![cfg(feature = "sqlite-vec")]

use std::sync::Arc;
use std::time::Duration;

use ai_memory::{
    MemoryPolicy, MemoryStore, RecallQuery, RememberRequest, SqliteStore, SqliteVecIndex,
};

#[test]
fn sqlite_vec_index_ranks_similar_text() {
    let mut policy = MemoryPolicy::default();
    policy.recall.time = 0.0;
    policy.recall.keyword = 0.0;
    policy.recall.vector = 1.0;
    policy.retention.working = Some(Duration::from_secs(60 * 60));

    let store = SqliteStore::builder()
        .in_memory()
        .vector_index(Arc::new(SqliteVecIndex::new().unwrap()))
        .build()
        .unwrap();
    store.create_project("p", policy).unwrap();
    store
        .remember(
            "p",
            RememberRequest::new("the quick brown fox jumps over the lazy dog"),
        )
        .unwrap();
    store
        .remember(
            "p",
            RememberRequest::new("unrelated chemistry lab protocol titration"),
        )
        .unwrap();

    let hits = store
        .recall(
            "p",
            RecallQuery::new("quick brown fox leaping dog").with_limit(2),
        )
        .unwrap();
    assert!(
        hits[0].memory.text.contains("fox"),
        "sqlite-vec path should rank similar fox text first: {:?}",
        hits.iter()
            .map(|h| (h.memory.text.clone(), h.vector_score))
            .collect::<Vec<_>>()
    );
    assert!(hits[0].vector_score > hits[1].vector_score);
}
