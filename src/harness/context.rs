use crate::types::{RecallHit, Tier};

use super::tokens::{CharsPer4, TokenBudget, TokenEstimator};

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
    /// Token estimate of [`Self::render`] under the estimator used to pack
    /// (default [`CharsPer4`] when packing without a custom estimator).
    pub tokens: usize,
}

impl ContextPack {
    pub fn from_hits(project_id: impl Into<String>, hits: &[RecallHit]) -> Self {
        let pack = Self {
            project_id: project_id.into(),
            blocks: hits.iter().map(block_from_hit).collect(),
            tokens: 0,
        };
        pack.with_tokens(&CharsPer4)
    }

    /// Pack hits so [`Self::render`] stays within `budget` under `estimator`.
    ///
    /// Order: pinned first, then hybrid score, then recency. Oversized lines are
    /// truncated (pins still get a stub if anything fits). Hits that cannot fit
    /// are skipped; packing continues so smaller later items can still land.
    pub fn from_hits_budgeted(
        project_id: impl Into<String>,
        hits: &[RecallHit],
        budget: TokenBudget,
        estimator: &dyn TokenEstimator,
    ) -> Self {
        let project_id = project_id.into();
        let mut pack = Self {
            project_id: project_id.clone(),
            blocks: Vec::new(),
            tokens: 0,
        };
        pack = pack.with_tokens(estimator);
        if pack.tokens > budget.max_tokens {
            return pack;
        }

        let mut ranked: Vec<&RecallHit> = hits.iter().collect();
        ranked.sort_by(|a, b| {
            b.memory
                .pinned
                .cmp(&a.memory.pinned)
                .then_with(|| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| b.memory.created_at.cmp(&a.memory.created_at))
        });

        for hit in ranked {
            if let Some(block) = fit_block(&pack, hit, budget, estimator) {
                pack.blocks.push(block);
                pack = pack.with_tokens(estimator);
            }
        }
        pack
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

    fn with_tokens(mut self, estimator: &dyn TokenEstimator) -> Self {
        self.tokens = estimator.tokens(&self.render());
        self
    }
}

impl std::fmt::Display for ContextPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

fn block_from_hit(h: &RecallHit) -> ContextBlock {
    ContextBlock {
        id: h.memory.id.clone(),
        tier: h.memory.tier,
        score: h.score,
        text: h.memory.text.clone(),
    }
}

fn estimate_with(
    pack: &ContextPack,
    extra: &ContextBlock,
    estimator: &dyn TokenEstimator,
) -> usize {
    let mut trial = pack.clone();
    trial.blocks.push(extra.clone());
    estimator.tokens(&trial.render())
}

fn fit_block(
    pack: &ContextPack,
    hit: &RecallHit,
    budget: TokenBudget,
    estimator: &dyn TokenEstimator,
) -> Option<ContextBlock> {
    let full = block_from_hit(hit);
    if estimate_with(pack, &full, estimator) <= budget.max_tokens {
        return Some(full);
    }

    let char_len = hit.memory.text.chars().count();
    if char_len == 0 {
        return None;
    }

    let mut lo = 0usize;
    let mut hi = char_len;
    let mut best: Option<ContextBlock> = None;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let mut text: String = hit.memory.text.chars().take(mid).collect();
        if mid < char_len {
            text.push('…');
        }
        let block = ContextBlock {
            id: hit.memory.id.clone(),
            tier: hit.memory.tier,
            score: hit.score,
            text,
        };
        if estimate_with(pack, &block, estimator) <= budget.max_tokens {
            best = Some(block);
            lo = mid.saturating_add(1);
        } else if mid == 0 {
            break;
        } else {
            hi = mid - 1;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Memory, RecallHit};
    use std::time::SystemTime;

    fn hit(id: &str, text: &str, score: f32, pinned: bool) -> RecallHit {
        RecallHit {
            memory: Memory {
                id: id.into(),
                project_id: "chat".into(),
                tier: Tier::Profile,
                text: text.into(),
                metadata: None,
                created_at: SystemTime::UNIX_EPOCH,
                updated_at: SystemTime::UNIX_EPOCH,
                last_accessed_at: None,
                access_count: 0,
                pinned,
            },
            score,
            time_score: 1.0,
            keyword_score: 0.5,
            vector_score: 0.9,
        }
    }

    #[test]
    fn render_cites_id_tier_score() {
        let pack = ContextPack::from_hits(
            "chat",
            &[hit("mem-1", "User prefers dark mode", 0.812, false)],
        );
        let s = pack.render();
        assert!(s.contains("project: chat"));
        assert!(s.contains("id=mem-1"));
        assert!(s.contains("[profile"));
        assert!(s.contains("0.812"));
        assert!(s.contains("User prefers dark mode"));
        assert_eq!(pack.tokens, CharsPer4.tokens(&s));
    }

    #[test]
    fn budgeted_pack_respects_chars_per_4() {
        let hits: Vec<_> = (0..20)
            .map(|i| {
                hit(
                    &format!("m{i}"),
                    &format!("filler line {i} {}", "word ".repeat(40)),
                    0.1,
                    false,
                )
            })
            .collect();
        let budget = TokenBudget::new(80);
        let pack = ContextPack::from_hits_budgeted("chat", &hits, budget, &CharsPer4);
        assert!(pack.tokens <= budget.max_tokens);
        assert_eq!(pack.tokens, CharsPer4.tokens(&pack.render()));
        assert!(!pack.blocks.is_empty());
    }

    #[test]
    fn pinned_survives_tight_budget() {
        let mut hits = vec![hit("pin", "keep-me-pin-token", 0.01, true)];
        for i in 0..8 {
            hits.push(hit(
                &format!("big{i}"),
                &"long unpinned noise ".repeat(30),
                0.99,
                false,
            ));
        }
        let budget = TokenBudget::new(64);
        let pack = ContextPack::from_hits_budgeted("chat", &hits, budget, &CharsPer4);
        assert!(pack.tokens <= budget.max_tokens);
        assert!(
            pack.blocks.iter().any(|b| b.id == "pin"),
            "pinned block must be packed first: {:?}",
            pack.blocks
        );
        assert!(
            pack.render().contains("keep-me-pin-token")
                || pack.blocks.iter().any(|b| b.id == "pin")
        );
    }
}
