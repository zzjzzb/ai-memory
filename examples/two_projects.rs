//! Two projects on one SQLite store, different [`ai_memory::MemoryPolicy`] values,
//! and recall that never crosses project boundaries.

use std::time::Duration;

use ai_memory::{open, MemoryPolicy, MemoryStore, RecallQuery, RememberRequest, Tier};

fn main() -> ai_memory::Result<()> {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("ai-memory-example-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let store = open(&path)?;

    let mut chat_policy = MemoryPolicy::chat();
    // Immediate promote so the demo can show policy-driven consolidation.
    chat_policy.promote.working_to_episodic_after = Duration::from_secs(0);
    chat_policy.promote.episodic_to_profile_after = Duration::from_secs(60 * 60 * 24 * 365);
    chat_policy.promote.episodic_to_profile_min_accesses = 99;

    let mut journal_policy = MemoryPolicy::journal();
    journal_policy.promote.working_to_episodic_after = Duration::from_secs(60 * 60 * 24 * 365);
    journal_policy.recall.keyword = 1.0;
    journal_policy.recall.vector = 0.0;
    journal_policy.recall.time = 0.0;

    store.create_project("chat-bot", chat_policy)?;
    store.create_project("journal", journal_policy)?;

    let chat = store.project("chat-bot")?;
    let journal = store.project("journal")?;

    chat.remember(
        RememberRequest::new("User prefers dark mode in the editor")
            .with_tier(Tier::Working)
            .with_metadata(serde_json::json!({"source": "settings"})),
    )?;
    chat.remember(
        RememberRequest::new("scratch: try the new sidebar layout").with_tier(Tier::Working),
    )?;

    journal.remember(
        RememberRequest::new("Today I hiked Mount Tam and saw the fog spill over the ridge")
            .with_tier(Tier::Working),
    )?;
    journal.remember(
        RememberRequest::new("pasta recipe: tomato basil garlic").with_tier(Tier::Working),
    )?;

    println!("== isolation: chat recalls 'dark mode', journal must not leak ==");
    let chat_hits = chat.recall(RecallQuery::new("dark mode preference").with_limit(5))?;
    print_hits("chat-bot", &chat_hits);
    let leaked = chat_hits
        .iter()
        .any(|h| h.memory.text.contains("Mount Tam"));
    println!("chat recall leaked journal text? {leaked}");

    let journal_hits = journal.recall(RecallQuery::new("hike fog ridge").with_limit(5))?;
    print_hits("journal", &journal_hits);
    let leaked = journal_hits
        .iter()
        .any(|h| h.memory.text.contains("dark mode"));
    println!("journal recall leaked chat text? {leaked}");

    println!("\n== consolidate: chat promotes working→episodic immediately; journal does not ==");
    let chat_report = chat.consolidate()?;
    let journal_report = journal.consolidate()?;
    println!(
        "chat consolidate: expired={} →episodic={} →profile={}",
        chat_report.expired, chat_report.promoted_to_episodic, chat_report.promoted_to_profile
    );
    println!(
        "journal consolidate: expired={} →episodic={} →profile={}",
        journal_report.expired,
        journal_report.promoted_to_episodic,
        journal_report.promoted_to_profile
    );

    println!("\nchat tiers after consolidate:");
    for m in chat.list_memories()? {
        println!("  [{}] {}", m.tier, m.text);
    }
    println!("journal tiers after consolidate:");
    for m in journal.list_memories()? {
        println!("  [{}] {}", m.tier, m.text);
    }

    println!("\nstore file: {}", path.display());
    Ok(())
}

fn print_hits(project: &str, hits: &[ai_memory::RecallHit]) {
    println!("{project} hits:");
    for (i, h) in hits.iter().enumerate() {
        println!(
            "  {}. score={:.3} (t={:.2} k={:.2} v={:.2}) [{}] {}",
            i + 1,
            h.score,
            h.time_score,
            h.keyword_score,
            h.vector_score,
            h.memory.tier,
            h.memory.text
        );
    }
}
