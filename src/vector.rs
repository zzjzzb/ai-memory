use crate::error::Result;

/// Similarity search over candidate embeddings.
///
/// MVP uses [`BruteForceCosine`] in-process. A later `sqlite-vec` (or Lance)
/// backend can implement this trait without changing [`crate::store::MemoryStore`].
pub trait VectorIndex: Send + Sync {
    /// Returns `(id, cosine)` for each candidate. Does not rank or truncate.
    fn similar(
        &self,
        query: &[f32],
        candidates: &[(String, Vec<f32>)],
    ) -> Result<Vec<(String, f32)>>;
}

/// In-crate brute-force cosine similarity. Fine for per-project MVP scale.
#[derive(Clone, Debug, Default)]
pub struct BruteForceCosine;

impl VectorIndex for BruteForceCosine {
    fn similar(
        &self,
        query: &[f32],
        candidates: &[(String, Vec<f32>)],
    ) -> Result<Vec<(String, f32)>> {
        Ok(candidates
            .iter()
            .map(|(id, vec)| (id.clone(), cosine(query, vec).max(0.0)))
            .collect())
    }
}

/// Cosine similarity in `[-1, 1]`. Length mismatch or zero vectors ⇒ `0.0`.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        (dot / denom).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_unit_vectors() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal() {
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }
}
