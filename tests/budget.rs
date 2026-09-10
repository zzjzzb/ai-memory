//! Budgeted ContextPack, pins under a tight budget, compaction, isolation.

use std::sync::Arc;

use ai_memory::{
    CharsPer4, ExtractiveCompactor, MemoryListFilter, MemoryPolicy, MemoryStore, RememberRequest,
    SqliteStore, Tier, TokenBudget, TokenEstimator,
};

fn store() -> SqliteStore {
    SqliteStore::builder()
        .in_memory()
        .embedder(Arc::new(ai_memory::HashEmbedder::new(32)))
        .build()
        .unwrap()
}

#[test]
fn budgeted_pack_never_exceeds_max_tokens() {
    let store = store();
    store
        .create_project("desk", MemoryPolicy::default())
        .unwrap();
    let session = store.session("desk").unwrap();
    for i in 0..40 {
        session
            .remember(
                RememberRequest::new(format!(
                    "working note {i} about the invoice queue and sidebar layout"
                ))
                .with_tier(Tier::Working),
            )
            .unwrap();
    }

    let budget = TokenBudget::new(96);
    let pack = session
        .prefetch_within_budget("invoice sidebar", budget)
        .unwrap();
    let est = CharsPer4.tokens(&pack.render());
    assert_eq!(pack.tokens, est);
    assert!(
        est <= budget.max_tokens,
        "pack tokens {est} exceeded budget {}",
        budget.max_tokens
    );
}

#[test]
fn pins_survive_tight_budget() {
    let store = store();
    store
        .create_project("desk", MemoryPolicy::default())
        .unwrap();
    let session = store.session("desk").unwrap();
    for i in 0..15 {
        session
            .remember(
                RememberRequest::new(format!(
                    "unpinned filler {i} {}",
                    "lorem ipsum dolor sit amet ".repeat(12)
                ))
                .with_tier(Tier::Working),
            )
            .unwrap();
    }
    let pin = session
        .remember(
            RememberRequest::new("PINNED-FACT-XYZ user billing owner is Ada")
                .with_tier(Tier::Profile),
        )
        .unwrap();
    session.store().pin("desk", &pin.id).unwrap();

    let budget = TokenBudget::new(80);
    let pack = session
        .prefetch_within_budget("billing owner Ada PINNED-FACT-XYZ", budget)
        .unwrap();
    assert!(pack.tokens <= budget.max_tokens);
    assert!(
        pack.blocks.iter().any(|b| b.id == pin.id) || pack.render().contains("PINNED-FACT-XYZ"),
        "pin must survive a tight budget, pack={}",
        pack.render()
    );
}

#[test]
fn budgeted_pack_stays_in_project() {
    let store = store();
    store
        .create_project("alpha", MemoryPolicy::default())
        .unwrap();
    store
        .create_project("beta", MemoryPolicy::default())
        .unwrap();
    let a = store.session("alpha").unwrap();
    let b = store.session("beta").unwrap();
    a.remember(RememberRequest::new("alpha-only secret phrase").with_tier(Tier::Profile))
        .unwrap();
    b.remember(RememberRequest::new("beta-only mango phrase").with_tier(Tier::Profile))
        .unwrap();

    let pack = a
        .prefetch_within_budget("secret mango", TokenBudget::new(256))
        .unwrap();
    assert!(pack.project_id == "alpha");
    assert!(!pack.render().contains("mango"));
    assert!(pack.blocks.iter().all(|bl| {
        a.store()
            .get("alpha", &bl.id)
            .unwrap()
            .is_some_and(|m| m.project_id == "alpha")
    }));
}

#[test]
fn compact_reduces_working_keeps_pinned() {
    let store = store();
    store
        .create_project("desk", MemoryPolicy::default())
        .unwrap();
    let session = store.session("desk").unwrap();
    for i in 0..12 {
        session
            .remember(RememberRequest::new(format!("scratch {i}")).with_tier(Tier::Working))
            .unwrap();
    }
    let pin = session
        .remember(RememberRequest::new("keep this working pin").with_tier(Tier::Working))
        .unwrap();
    session.store().pin("desk", &pin.id).unwrap();

    let before = session
        .store()
        .list_memories_filtered(
            "desk",
            MemoryListFilter::new().with_tiers(vec![Tier::Working]),
        )
        .unwrap()
        .len();
    assert_eq!(before, 13);

    let report = session
        .compact_working_with(&ExtractiveCompactor::new(2))
        .unwrap();
    assert_eq!(report.kept_working, 3); // 1 pin + 2 newest unpinned
    assert_eq!(report.folded_working, 10);
    assert_eq!(report.forgotten_working, 10);
    assert!(report.episodic_id.is_some());

    let working = session
        .store()
        .list_memories_filtered(
            "desk",
            MemoryListFilter::new().with_tiers(vec![Tier::Working]),
        )
        .unwrap();
    assert_eq!(working.len(), 3);
    assert!(working.iter().any(|m| m.id == pin.id && m.pinned));

    let folded = session
        .store()
        .get("desk", report.episodic_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(folded.tier, Tier::Episodic);
    assert!(folded.text.contains("Folded working notes"));
}

#[test]
fn compact_does_not_touch_other_project() {
    let store = store();
    store
        .create_project("alpha", MemoryPolicy::default())
        .unwrap();
    store
        .create_project("beta", MemoryPolicy::default())
        .unwrap();
    let a = store.session("alpha").unwrap();
    let b = store.session("beta").unwrap();
    for i in 0..6 {
        a.remember(RememberRequest::new(format!("a-{i}")).with_tier(Tier::Working))
            .unwrap();
        b.remember(RememberRequest::new(format!("b-{i}")).with_tier(Tier::Working))
            .unwrap();
    }
    a.compact_working_with(&ExtractiveCompactor::new(1))
        .unwrap();
    let beta_working = store
        .list_memories_filtered(
            "beta",
            MemoryListFilter::new().with_tiers(vec![Tier::Working]),
        )
        .unwrap();
    assert_eq!(beta_working.len(), 6);
}
