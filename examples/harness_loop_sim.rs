//! Simulated agent harness loop (no LLM, no network).
//!
//! Registers JSON tool specs, prefetches memory, packs context, runs fake tool
//! calls, consolidates. Two projects share one SQLite store.
//!
//! ```bash
//! cargo run --example harness_loop_sim
//! ```

use std::sync::Arc;

use ai_memory::{
    memory_tool_specs, open_in_memory_with_embedder, HashEmbedder, MemoryPolicy, MemoryStore,
    RememberRequest, Tier, TOOL_CONSOLIDATE, TOOL_RECALL, TOOL_REMEMBER,
};
use serde_json::json;

fn main() -> ai_memory::Result<()> {
    let store = open_in_memory_with_embedder(Arc::new(HashEmbedder::new(64)))?;
    store.create_project("support-bot", MemoryPolicy::chat())?;
    store.create_project("journal", MemoryPolicy::journal())?;

    let support = store.session("support-bot")?;
    let journal = store.session("journal")?;

    println!("== register tools with your harness ==");
    for spec in memory_tool_specs() {
        println!(
            "- {} (openai type={})",
            spec.name,
            spec.openai_tool()["type"]
        );
    }

    println!("\n== turn 1 (support-bot): prefetch empty, then remember via tools ==");
    let hits = support.prefetch("what theme does the user like?")?;
    println!("{}", support.pack_context(&hits).render());
    let r = support.call_tool(
        TOOL_REMEMBER,
        json!({"text": "User prefers dark mode in the editor", "tier": "profile"}),
    );
    println!("tool {} ok={} data={}", r.name, r.ok, r.data);
    support.remember_turn([
        RememberRequest::new("User: the sidebar is still broken").with_tier(Tier::Working),
        RememberRequest::new("Assistant: I'll check memory and the layout notes")
            .with_tier(Tier::Working),
    ])?;

    println!(
        "\n== turn 2 (support-bot): budgeted pack before 'model' (not the full transcript) =="
    );
    let pack =
        support.prefetch_within_budget("theme and sidebar", ai_memory::TokenBudget::new(256))?;
    println!("{}", pack.render());
    println!("pack tokens (chars/4)={} budget=256", pack.tokens);
    let rec = support.call_tool(TOOL_RECALL, json!({"text": "dark mode", "limit": 3}));
    println!("recall ok={} hits={}", rec.ok, rec.data["hits"]);
    let _ = support.call_tool(TOOL_CONSOLIDATE, json!({}));
    let report = support.end_turn_consolidate()?;
    println!(
        "consolidate expired={} →episodic={} →profile={}",
        report.expired, report.promoted_to_episodic, report.promoted_to_profile
    );

    println!("\n== turn 3 (journal): same DB file, isolated project ==");
    journal.remember_turn([
        RememberRequest::new("Today I hiked Mount Tam and watched the fog")
            .with_tier(Tier::Working),
    ])?;
    let leaked = journal
        .prefetch("dark mode editor")?
        .iter()
        .any(|h| h.memory.text.to_lowercase().contains("dark mode"));
    println!("journal prefetch leaked support-bot dark-mode fact? {leaked}");
    println!(
        "{}",
        journal
            .pack_context(&journal.prefetch("hike fog")?)
            .render()
    );

    println!("\ndone.");
    Ok(())
}
