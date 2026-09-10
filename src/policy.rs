use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Per-project memory policy. Storage engine is shared; policies differ.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryPolicy {
    pub retention: RetentionPolicy,
    pub promote: PromotePolicy,
    pub recall: RecallWeights,
}

impl MemoryPolicy {
    pub fn validate(&self) -> Result<()> {
        self.recall.validate()?;
        Ok(())
    }

    /// Chat-style: strong vector recall, short working memory.
    pub fn chat() -> Self {
        let mut p = Self::default();
        p.retention.working = Some(Duration::from_secs(60 * 60));
        p.recall.time = 0.15;
        p.recall.keyword = 0.25;
        p.recall.vector = 0.60;
        p
    }

    /// Journal-style: keyword-heavy recall, faster promotion into episodic.
    pub fn journal() -> Self {
        let mut p = Self::default();
        p.promote.working_to_episodic_after = Duration::from_secs(60);
        p.recall.time = 0.20;
        p.recall.keyword = 0.65;
        p.recall.vector = 0.15;
        p
    }
}

/// TTL hints per tier. `None` means keep indefinitely.
///
/// Unpinned memories whose age is `>=` the TTL are deleted on consolidate.
/// Set retention **longer** than the corresponding promote delay, or items
/// expire before they can move up a tier.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    #[serde(with = "option_duration_secs")]
    pub working: Option<Duration>,
    #[serde(with = "option_duration_secs")]
    pub episodic: Option<Duration>,
    #[serde(with = "option_duration_secs")]
    pub profile: Option<Duration>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            working: Some(Duration::from_secs(24 * 60 * 60)),
            episodic: Some(Duration::from_secs(30 * 24 * 60 * 60)),
            profile: None,
        }
    }
}

impl RetentionPolicy {
    pub fn ttl(&self, tier: crate::types::Tier) -> Option<Duration> {
        match tier {
            crate::types::Tier::Working => self.working,
            crate::types::Tier::Episodic => self.episodic,
            crate::types::Tier::Profile => self.profile,
        }
    }
}

/// Working → episodic → profile promotion rules applied by `consolidate`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PromotePolicy {
    #[serde(with = "duration_secs")]
    pub working_to_episodic_after: Duration,
    #[serde(with = "duration_secs")]
    pub episodic_to_profile_after: Duration,
    /// Additional bar for episodic → profile (recall increments `access_count`).
    pub episodic_to_profile_min_accesses: u32,
    /// Pinned rows skip retention expiry (they can still promote).
    pub pinned_skip_expiry: bool,
}

impl Default for PromotePolicy {
    fn default() -> Self {
        Self {
            working_to_episodic_after: Duration::from_secs(60 * 60),
            episodic_to_profile_after: Duration::from_secs(7 * 24 * 60 * 60),
            episodic_to_profile_min_accesses: 2,
            pinned_skip_expiry: true,
        }
    }
}

/// Hybrid recall knobs. Weights are L1-normalized at score time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecallWeights {
    pub time: f32,
    pub keyword: f32,
    pub vector: f32,
    /// Age at which the recency term is 0.5 (exponential half-life).
    #[serde(with = "duration_secs")]
    pub time_half_life: Duration,
}

impl Default for RecallWeights {
    fn default() -> Self {
        Self {
            time: 0.30,
            keyword: 0.30,
            vector: 0.40,
            time_half_life: Duration::from_secs(7 * 24 * 60 * 60),
        }
    }
}

impl RecallWeights {
    pub fn validate(&self) -> Result<()> {
        for (name, w) in [
            ("time", self.time),
            ("keyword", self.keyword),
            ("vector", self.vector),
        ] {
            if !w.is_finite() || w < 0.0 {
                return Err(Error::InvalidPolicy(format!(
                    "recall.{name} must be a finite number >= 0"
                )));
            }
        }
        if self.time + self.keyword + self.vector <= 0.0 {
            return Err(Error::InvalidPolicy(
                "at least one recall weight must be > 0".into(),
            ));
        }
        if self.time_half_life.as_secs() == 0 {
            return Err(Error::InvalidPolicy(
                "recall.time_half_life must be >= 1s".into(),
            ));
        }
        Ok(())
    }

    pub fn normalized(&self) -> (f32, f32, f32) {
        let s = (self.time + self.keyword + self.vector).max(1e-6);
        (self.time / s, self.keyword / s, self.vector / s)
    }
}

pub(crate) fn hybrid_score(
    weights: &RecallWeights,
    age: Duration,
    keyword_score: f32,
    vector_score: f32,
) -> (f32, f32, f32, f32) {
    let (wt, wk, wv) = weights.normalized();
    let time_score = recency_score(age, weights.time_half_life);
    let score = wt * time_score + wk * keyword_score + wv * vector_score;
    (score, time_score, keyword_score, vector_score)
}

pub(crate) fn recency_score(age: Duration, half_life: Duration) -> f32 {
    let hl = half_life.as_secs_f64().max(1.0);
    let a = age.as_secs_f64().max(0.0);
    ((-a / hl) * std::f64::consts::LN_2).exp() as f32
}

pub(crate) fn keyword_score(query_tokens: &[String], doc_tokens: &[String]) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }
    let q: std::collections::HashSet<&String> = query_tokens.iter().collect();
    let d: std::collections::HashSet<&String> = doc_tokens.iter().collect();
    if q.is_empty() {
        return 0.0;
    }
    let overlap = q.intersection(&d).count() as f32;
    overlap / q.len() as f32
}

mod duration_secs {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(d: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(d.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

mod option_duration_secs {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(d: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match d {
            Some(dur) => serializer.serialize_some(&dur.as_secs()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = Option::<u64>::deserialize(deserializer)?;
        Ok(secs.map(Duration::from_secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_roundtrips_json() {
        let p = MemoryPolicy::default();
        let s = serde_json::to_string(&p).unwrap();
        let q: MemoryPolicy = serde_json::from_str(&s).unwrap();
        assert_eq!(p, q);
    }

    #[test]
    fn vector_weight_changes_ranking() {
        let vectorish = RecallWeights {
            time: 0.0,
            keyword: 0.0,
            vector: 1.0,
            ..RecallWeights::default()
        };

        let keywordish = RecallWeights {
            time: 0.0,
            keyword: 1.0,
            vector: 0.0,
            ..RecallWeights::default()
        };

        let age = Duration::from_secs(0);
        let (vec_low_kw, ..) = hybrid_score(&vectorish, age, 1.0, 0.1);
        let (vec_high_vec, ..) = hybrid_score(&vectorish, age, 0.0, 0.9);
        assert!(vec_high_vec > vec_low_kw);

        let (kw_high_kw, ..) = hybrid_score(&keywordish, age, 1.0, 0.1);
        let (kw_high_vec, ..) = hybrid_score(&keywordish, age, 0.0, 0.9);
        assert!(kw_high_kw > kw_high_vec);
    }
}
