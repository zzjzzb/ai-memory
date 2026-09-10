use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::embedder::Embedder;
use crate::error::{Error, Result};

/// Default in-process embedding LRU (text + dim). Shared across projects.
pub const EMBED_CACHE_CAPACITY: usize = 2048;

/// Wraps an [`Embedder`] so identical `(dim, text)` is computed once per process store.
pub(crate) struct CachedEmbedder {
    inner: Arc<dyn Embedder>,
    cache: Mutex<Lru>,
}

impl CachedEmbedder {
    pub(crate) fn wrap(inner: Arc<dyn Embedder>) -> Self {
        Self {
            inner,
            cache: Mutex::new(Lru::new(EMBED_CACHE_CAPACITY)),
        }
    }
}

impl Embedder for CachedEmbedder {
    fn dim(&self) -> usize {
        self.inner.dim()
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let dim = self.inner.dim();
        {
            let mut lru = self.cache.lock().map_err(|_| Error::Poisoned)?;
            if let Some(hit) = lru.get(dim, text) {
                return Ok(hit);
            }
        }
        let v = self.inner.embed(text)?;
        if let Ok(mut lru) = self.cache.lock() {
            lru.put(dim, text, v.clone());
        }
        Ok(v)
    }
}

struct Lru {
    cap: usize,
    map: HashMap<(usize, String), Vec<f32>>,
    order: VecDeque<(usize, String)>,
}

impl Lru {
    fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, dim: usize, text: &str) -> Option<Vec<f32>> {
        let key = (dim, text.to_string());
        let v = self.map.get(&key)?.clone();
        if let Some(i) = self.order.iter().position(|k| k == &key) {
            self.order.remove(i);
        }
        self.order.push_back(key);
        Some(v)
    }

    fn put(&mut self, dim: usize, text: &str, value: Vec<f32>) {
        let key = (dim, text.to_string());
        if self.map.contains_key(&key) {
            self.map.insert(key.clone(), value);
            if let Some(i) = self.order.iter().position(|k| k == &key) {
                self.order.remove(i);
            }
            self.order.push_back(key);
            return;
        }
        while self.map.len() >= self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            } else {
                break;
            }
        }
        self.map.insert(key.clone(), value);
        self.order.push_back(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::HashEmbedder;

    #[test]
    fn lru_evicts_oldest() {
        let mut lru = Lru::new(2);
        lru.put(8, "a", vec![1.0]);
        lru.put(8, "b", vec![2.0]);
        lru.put(8, "c", vec![3.0]);
        assert!(lru.get(8, "a").is_none());
        assert_eq!(lru.get(8, "b").unwrap()[0], 2.0);
        assert_eq!(lru.get(8, "c").unwrap()[0], 3.0);
    }

    #[test]
    fn cached_embedder_matches_inner() {
        let inner = Arc::new(HashEmbedder::new(16));
        let cached = CachedEmbedder::wrap(inner.clone());
        assert_eq!(
            cached.embed("same text").unwrap(),
            inner.embed("same text").unwrap()
        );
    }
}
