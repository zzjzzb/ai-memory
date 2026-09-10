use crate::types::Memory;

/// Which working rows to keep vs fold. Produced by a [`Compactor`].
#[derive(Clone, Debug, Default)]
pub struct CompactPlan {
    pub keep_ids: Vec<String>,
    pub fold: Vec<Memory>,
}

/// Offline (or later LLM) planner for shrinking working memory.
///
/// The default [`ExtractiveCompactor`] does not call a model. An LLM
/// implementation can fold text differently; it must still only return ids + text.
pub trait Compactor: Send + Sync {
    fn plan(&self, working: &[Memory]) -> CompactPlan;

    fn folded_text(&self, fold: &[Memory]) -> Option<String> {
        ExtractiveCompactor::default().folded_text(fold)
    }
}

/// Keep pinned working rows + the N newest unpinned ones. Fold the rest into
/// one extractive episodic note (concatenated snippets, not generated prose).
#[derive(Clone, Debug)]
pub struct ExtractiveCompactor {
    /// Unpinned working rows to leave in place (newest first). Pinned are extra.
    pub keep_unpinned: usize,
    /// Max characters copied from each folded row into the episodic note.
    pub snippet_chars: usize,
}

impl Default for ExtractiveCompactor {
    fn default() -> Self {
        Self {
            keep_unpinned: 8,
            snippet_chars: 240,
        }
    }
}

impl ExtractiveCompactor {
    pub fn new(keep_unpinned: usize) -> Self {
        Self {
            keep_unpinned,
            ..Self::default()
        }
    }

    pub fn folded_text(&self, fold: &[Memory]) -> Option<String> {
        if fold.is_empty() {
            return None;
        }
        let mut out = String::from("Folded working notes (extractive):\n");
        for m in fold {
            let snip: String = m.text.chars().take(self.snippet_chars).collect();
            out.push_str("- ");
            out.push_str(&m.id);
            out.push_str(": ");
            out.push_str(&snip);
            out.push('\n');
        }
        Some(out)
    }
}

impl Compactor for ExtractiveCompactor {
    fn plan(&self, working: &[Memory]) -> CompactPlan {
        let mut pinned = Vec::new();
        let mut unpinned = Vec::new();
        for m in working {
            if m.pinned {
                pinned.push(m.clone());
            } else {
                unpinned.push(m.clone());
            }
        }
        unpinned.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        let keep_u = unpinned.len().min(self.keep_unpinned);
        let keep_unpinned = unpinned.iter().take(keep_u).cloned().collect::<Vec<_>>();
        let fold = unpinned.into_iter().skip(keep_u).collect::<Vec<_>>();

        let mut keep_ids: Vec<String> = pinned.into_iter().map(|m| m.id).collect();
        keep_ids.extend(keep_unpinned.into_iter().map(|m| m.id));
        CompactPlan { keep_ids, fold }
    }

    fn folded_text(&self, fold: &[Memory]) -> Option<String> {
        ExtractiveCompactor::folded_text(self, fold)
    }
}

/// Outcome of [`super::AgentSession::compact_working`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompactReport {
    pub kept_working: usize,
    pub folded_working: usize,
    pub forgotten_working: usize,
    pub episodic_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Tier;
    use std::time::{Duration, SystemTime};

    fn mem(id: &str, pinned: bool, age_secs: u64) -> Memory {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 - age_secs);
        Memory {
            id: id.into(),
            project_id: "p".into(),
            tier: Tier::Working,
            text: format!("note {id}"),
            metadata: None,
            created_at: t,
            updated_at: t,
            last_accessed_at: None,
            access_count: 0,
            pinned,
        }
    }

    #[test]
    fn keeps_pins_and_newest_unpinned() {
        let working = vec![
            mem("old", false, 90),
            mem("mid", false, 50),
            mem("new", false, 10),
            mem("pin", true, 80),
        ];
        let plan = ExtractiveCompactor::new(1).plan(&working);
        assert!(plan.keep_ids.contains(&"pin".into()));
        assert!(plan.keep_ids.contains(&"new".into()));
        assert_eq!(plan.fold.len(), 2);
        assert!(plan.fold.iter().any(|m| m.id == "old"));
    }
}
