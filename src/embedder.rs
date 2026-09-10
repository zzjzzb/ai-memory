use crate::error::{Error, Result};
use crate::util::{l2_normalize, tokenize};

/// Turns text into a dense vector. Production callers plug in a real model;
/// tests and the default store use [`HashEmbedder`] (no network, no API keys).
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

/// Deterministic bag-of-tokens hash embedding.
///
/// Similar token overlap ⇒ higher cosine similarity. Not a substitute for a
/// real encoder, but enough to exercise the vector recall path in CI.
#[derive(Clone, Debug)]
pub struct HashEmbedder {
    dim: usize,
}

impl HashEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim: dim.max(8) }
    }
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self::new(64)
    }
}

impl Embedder for HashEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut v = vec![0.0f32; self.dim];
        let tokens = tokenize(text);
        if tokens.is_empty() {
            v[0] = 1.0;
            return Ok(v);
        }
        for token in &tokens {
            let h = fnv1a64(token.as_bytes());
            let idx = (h as usize) % self.dim;
            let sign = if h & 1 == 0 { 1.0 } else { -1.0 };
            v[idx] += sign;
            let idx2 = ((h >> 32) as usize) % self.dim;
            let sign2 = if (h >> 1) & 1 == 0 { 1.0 } else { -1.0 };
            v[idx2] += 0.5 * sign2;
        }
        if !l2_normalize(&mut v) {
            return Err(Error::Embedding("zero embedding".into()));
        }
        Ok(v)
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::cosine;

    #[test]
    fn deterministic() {
        let e = HashEmbedder::new(32);
        assert_eq!(
            e.embed("hello world").unwrap(),
            e.embed("hello world").unwrap()
        );
    }

    #[test]
    fn similar_text_outranks_unrelated() {
        let e = HashEmbedder::new(64);
        let q = e.embed("the quick brown fox jumps").unwrap();
        let near = e.embed("quick brown fox leaping").unwrap();
        let far = e.embed("lasagna recipe tomato basil").unwrap();
        assert!(cosine(&q, &near) > cosine(&q, &far));
    }
}
