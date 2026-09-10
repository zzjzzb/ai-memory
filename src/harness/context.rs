use crate::types::{RecallHit, Tier};

/// One cited memory line inside a [`ContextPack`].
#[derive(Clone, Debug, PartialEq)]
pub struct ContextBlock {
    pub id: String,
    pub tier: Tier,
    pub score: f32,
    pub text: String,
}

/// Stable, model-ready memory context (system prompt or tool result).
#[derive(Clone, Debug, PartialEq)]
pub struct ContextPack {
    pub project_id: String,
    pub blocks: Vec<ContextBlock>,
}

impl ContextPack {
    pub fn from_hits(project_id: impl Into<String>, hits: &[RecallHit]) -> Self {
        let blocks = hits
            .iter()
            .map(|h| ContextBlock {
                id: h.memory.id.clone(),
                tier: h.memory.tier,
                score: h.score,
                text: h.memory.text.clone(),
            })
            .collect();
        Self {
            project_id: project_id.into(),
            blocks,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Deterministic text for a system/developer message.
    pub fn render(&self) -> String {
        if self.blocks.is_empty() {
            return format!("## Memory (project: {}, 0 hits)\n(none)", self.project_id);
        }
        let mut out = format!(
            "## Memory (project: {}, {} hits)\n",
            self.project_id,
            self.blocks.len()
        );
        for b in &self.blocks {
            out.push_str(&format!(
                "- [{} id={} score={:.3}] {}\n",
                b.tier, b.id, b.score, b.text
            ));
        }
        out
    }
}

impl std::fmt::Display for ContextPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Memory, RecallHit};
    use std::time::SystemTime;

    #[test]
    fn render_cites_id_tier_score() {
        let hit = RecallHit {
            memory: Memory {
                id: "mem-1".into(),
                project_id: "chat".into(),
                tier: Tier::Profile,
                text: "User prefers dark mode".into(),
                metadata: None,
                created_at: SystemTime::UNIX_EPOCH,
                updated_at: SystemTime::UNIX_EPOCH,
                last_accessed_at: None,
                access_count: 0,
                pinned: false,
            },
            score: 0.812,
            time_score: 1.0,
            keyword_score: 0.5,
            vector_score: 0.9,
        };
        let pack = ContextPack::from_hits("chat", &[hit]);
        let s = pack.render();
        assert!(s.contains("project: chat"));
        assert!(s.contains("id=mem-1"));
        assert!(s.contains("[profile"));
        assert!(s.contains("0.812"));
        assert!(s.contains("User prefers dark mode"));
    }
}
