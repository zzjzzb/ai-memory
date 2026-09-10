/// How many tokens a string is allowed to use in a [`super::ContextPack`].
///
/// Typical harness budgets are 2k–32k. This is **not** the model context window
/// (often ~1M). You store the long session; you pack a slice per turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenBudget {
    pub max_tokens: usize,
}

impl TokenBudget {
    /// Default pack size (~8k tokens under [`CharsPer4`]).
    pub const DEFAULT_MAX: usize = 8_192;

    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX)
    }
}

/// Estimates tokens for budgeted packing. Default is [`CharsPer4`] (no tiktoken).
pub trait TokenEstimator: Send + Sync {
    fn tokens(&self, text: &str) -> usize;
}

/// `ceil(chars / 4)`. Unicode scalar values, not bytes. Offline, no tokenizer crate.
#[derive(Clone, Copy, Debug, Default)]
pub struct CharsPer4;

impl TokenEstimator for CharsPer4 {
    fn tokens(&self, text: &str) -> usize {
        let n = text.chars().count();
        n.div_ceil(4)
    }
}

/// How many recall hits to fetch before packing into `budget`.
pub(crate) fn prefetch_hit_limit(max_tokens: usize) -> usize {
    (max_tokens / 24).clamp(16, 128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chars_per_4_rounds_up() {
        assert_eq!(CharsPer4.tokens(""), 0);
        assert_eq!(CharsPer4.tokens("abcd"), 1);
        assert_eq!(CharsPer4.tokens("abcde"), 2);
    }
}
