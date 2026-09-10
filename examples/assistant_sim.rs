//! Simulated personal assistant using `ai-memory` across two projects.
//!
//! No network. Default [`ai_memory::HashEmbedder`].
//!
//! ```bash
//! cargo run --example assistant_sim
//! ```

use std::sync::Arc;
use std::time::Duration;

use ai_memory::{
    open_in_memory_with_embedder, HashEmbedder, MemoryPolicy, MemoryStore, RecallQuery,
    RememberRequest, Tier,
};

fn main() -> ai_memory::Result<()> {
    let store = open_in_memory_with_embedder(Arc::new(HashEmbedder::new(64)))?;
    println!(
        "store: in-memory SQLite, embedder dim={}",
        store.embedder().dim()
    );

    let mut chat_policy = MemoryPolicy::chat();
    chat_policy.promote.working_to_episodic_after = Duration::from_secs(0);
    chat_policy.retention.working = Some(Duration::from_secs(24 * 60 * 60));

    let mut journal_policy = MemoryPolicy::journal();
    journal_policy.promote.working_to_episodic_after = Duration::from_secs(60 * 60 * 24 * 365);
    journal_policy.recall.keyword = 0.7;
    journal_policy.recall.vector = 0.2;
    journal_policy.recall.time = 0.1;

    store.create_project("chat", chat_policy)?;
    store.create_project("journal", journal_policy)?;
    let chat = store.project("chat")?;
    let journal = store.project("journal")?;

    println!("\n== turn 1: remember ==");
    chat.remember(
        RememberRequest::new("User said: let's debug the sidebar tomorrow")
            .with_tier(Tier::Working)
            .with_metadata(serde_json::json!({"role": "user"})),
    )?;
    chat.remember(
        RememberRequest::new("User prefers dark mode in the editor").with_tier(Tier::Profile),
    )?;
    journal.remember(
        RememberRequest::new("Today I hiked Mount Tam and watched the fog spill over the ridge")
            .with_tier(Tier::Working),
    )?;
    journal.remember(
        RememberRequest::new("pasta recipe: tomato basil garlic").with_tier(Tier::Working),
    )?;

    println!("chat memories: {}", chat.list_memories()?.len());
    println!("journal memories: {}", journal.list_memories()?.len());

    println!("\n== turn 2: assistant answers from recall (chat) ==");
    answer(&chat, "What theme does the user like?")?;
    answer(&chat, "sidebar plans")?;

    println!("\n== turn 3: journal recall must not leak chat facts ==");
    let journal_hits = journal.recall(RecallQuery::new("dark mode editor").with_limit(5))?;
    let leaked = journal_hits
        .iter()
        .any(|h| h.memory.text.to_lowercase().contains("dark mode"));
    println!("journal asked about dark mode; leaked chat fact? {leaked}");
    print_hits("journal", &journal_hits);

    println!("\n== turn 4: consolidate (chat promotes working→episodic; journal does not) ==");
    let cr = chat.consolidate()?;
    let jr = journal.consolidate()?;
    println!(
        "chat  expired={} →episodic={} →profile={}",
        cr.expired, cr.promoted_to_episodic, cr.promoted_to_profile
    );
    println!(
        "journal expired={} →episodic={} →profile={}",
        jr.expired, jr.promoted_to_episodic, jr.promoted_to_profile
    );
    println!("chat tiers:");
    for m in chat.list_memories()? {
        println!("  [{}] {}", m.tier, m.text);
    }
    println!("journal tiers:");
    for m in journal.list_memories()? {
        println!("  [{}] {}", m.tier, m.text);
    }

    println!("\n== turn 5: pin + forget ==");
    let pasta = journal
        .list_memories()?
        .into_iter()
        .find(|m| m.text.contains("pasta"))
        .expect("pasta note");
    journal.forget(&pasta.id)?;
    println!("forgot journal pasta note; remaining:");
    for m in journal.list_memories()? {
        println!("  [{}] {}", m.tier, m.text);
    }

    println!("\nprojects: {:?}", {
        store
            .list_projects()?
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
    });
    Ok(())
}

fn answer(
    project: &ai_memory::ProjectHandle<ai_memory::SqliteStore>,
    question: &str,
) -> ai_memory::Result<()> {
    println!("Q: {question}");
    let hits = project.recall(RecallQuery::new(question).with_limit(3))?;
    if hits.is_empty() {
        println!("A: (no memories)");
        return Ok(());
    }
    println!("A: {}", hits[0].memory.text);
    print_hits(project.id(), &hits);
    Ok(())
}

fn print_hits(label: &str, hits: &[ai_memory::RecallHit]) {
    for (i, h) in hits.iter().enumerate() {
        println!(
            "  {label} #{i} score={:.3} (t={:.2} k={:.2} v={:.2}) [{}] {}",
            h.score, h.time_score, h.keyword_score, h.vector_score, h.memory.tier, h.memory.text
        );
    }
}
