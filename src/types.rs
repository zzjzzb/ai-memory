use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

/// Memory durability tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Session / ephemeral notes.
    Working,
    /// Day / event logs.
    Episodic,
    /// Durable long-term facts.
    Profile,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Working => "working",
            Tier::Episodic => "episodic",
            Tier::Profile => "profile",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "working" => Ok(Tier::Working),
            "episodic" => Ok(Tier::Episodic),
            "profile" => Ok(Tier::Profile),
            other => Err(Error::InvalidTier(other.to_string())),
        }
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A stored memory row. Always belongs to exactly one `project`.
#[derive(Clone, Debug, PartialEq)]
pub struct Memory {
    pub id: String,
    pub project_id: String,
    pub tier: Tier,
    pub text: String,
    pub metadata: Option<Value>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub last_accessed_at: Option<SystemTime>,
    pub access_count: u32,
    pub pinned: bool,
}

/// Project record (id + policy).
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub id: String,
    pub policy: crate::policy::MemoryPolicy,
    pub created_at: SystemTime,
}

/// Input to [`crate::store::MemoryStore::remember`].
#[derive(Clone, Debug, Default)]
pub struct RememberRequest {
    pub text: String,
    pub metadata: Option<Value>,
    /// When `None`, [`crate::heuristic::infer_tier`] chooses a tier.
    pub tier: Option<Tier>,
}

impl RememberRequest {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            metadata: None,
            tier: None,
        }
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_tier(mut self, tier: Tier) -> Self {
        self.tier = Some(tier);
        self
    }
}

impl From<&str> for RememberRequest {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for RememberRequest {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

/// Hybrid recall query, always scoped to a single project by the store API.
#[derive(Clone, Debug)]
pub struct RecallQuery {
    pub text: String,
    pub limit: usize,
    /// Inclusive lower bound on `created_at`.
    pub since: Option<SystemTime>,
    /// Inclusive upper bound on `created_at`.
    pub until: Option<SystemTime>,
    /// Restrict to these tiers. `None` means all tiers.
    pub tiers: Option<Vec<Tier>>,
    /// Drop hits below this combined score. `None` means no cutoff.
    pub min_score: Option<f32>,
}

impl RecallQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            limit: 8,
            since: None,
            until: None,
            tiers: None,
            min_score: None,
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_since(mut self, since: SystemTime) -> Self {
        self.since = Some(since);
        self
    }

    pub fn with_until(mut self, until: SystemTime) -> Self {
        self.until = Some(until);
        self
    }

    pub fn with_tiers(mut self, tiers: Vec<Tier>) -> Self {
        self.tiers = Some(tiers);
        self
    }

    pub fn with_min_score(mut self, min_score: f32) -> Self {
        self.min_score = Some(min_score);
        self
    }
}

/// Ranked recall result with component scores.
#[derive(Clone, Debug)]
pub struct RecallHit {
    pub memory: Memory,
    pub score: f32,
    pub time_score: f32,
    pub keyword_score: f32,
    pub vector_score: f32,
}

/// Filters for [`crate::store::MemoryStore::list_memories_filtered`].
///
/// Default: all tiers, no time window, exclude expired unpinned rows.
#[derive(Clone, Debug, Default)]
pub struct MemoryListFilter {
    pub tiers: Option<Vec<Tier>>,
    pub since: Option<SystemTime>,
    pub until: Option<SystemTime>,
    /// `Some(true)` = pinned only, `Some(false)` = unpinned only, `None` = both.
    pub pinned: Option<bool>,
    /// When false (default), rows past the project's retention TTL are omitted
    /// unless pin-protected — matching recall.
    pub include_expired: bool,
    pub limit: Option<usize>,
}

impl MemoryListFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tiers(mut self, tiers: Vec<Tier>) -> Self {
        self.tiers = Some(tiers);
        self
    }

    pub fn with_since(mut self, since: SystemTime) -> Self {
        self.since = Some(since);
        self
    }

    pub fn with_until(mut self, until: SystemTime) -> Self {
        self.until = Some(until);
        self
    }

    pub fn pinned_only(mut self) -> Self {
        self.pinned = Some(true);
        self
    }

    pub fn unpinned_only(mut self) -> Self {
        self.pinned = Some(false);
        self
    }

    pub fn including_expired(mut self) -> Self {
        self.include_expired = true;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Outcome of [`crate::store::MemoryStore::consolidate`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsolidateReport {
    pub expired: u64,
    pub promoted_to_episodic: u64,
    pub promoted_to_profile: u64,
}
