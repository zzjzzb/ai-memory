//! sqlite-vec-backed [`VectorIndex`]. Only compiled with `--features sqlite-vec`.

use std::sync::{Mutex, Once};

use rusqlite::{params, Connection};

use crate::error::{Error, Result};
use crate::util::vec_to_blob;
use crate::vector::VectorIndex;

fn register_sqlite_vec() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // sqlite-vec's init symbol does not match rusqlite's callback type
        // exactly; this is the supported registration path (v0.1.6 + rusqlite 0.32).
        #[allow(clippy::missing_transmute_annotations)]
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// Cosine similarity via sqlite-vec `vec_distance_cosine` (distance 0 = identical).
///
/// Inject with `SqliteStore::builder().vector_index(Arc::new(SqliteVecIndex::new()?))`.
/// Default stores still use [`crate::vector::BruteForceCosine`].
pub struct SqliteVecIndex {
    conn: Mutex<Connection>,
}

impl SqliteVecIndex {
    pub fn new() -> Result<Self> {
        register_sqlite_vec();
        let conn = Connection::open_in_memory()?;
        // Fail fast if the extension did not load.
        let _: String = conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn version(&self) -> Result<String> {
        let conn = self.conn.lock().map_err(|_| Error::Poisoned)?;
        Ok(conn.query_row("SELECT vec_version()", [], |row| row.get(0))?)
    }
}

impl VectorIndex for SqliteVecIndex {
    fn similar(
        &self,
        query: &[f32],
        candidates: &[(String, Vec<f32>)],
    ) -> Result<Vec<(String, f32)>> {
        let conn = self.conn.lock().map_err(|_| Error::Poisoned)?;
        let q = vec_to_blob(query);
        let mut out = Vec::with_capacity(candidates.len());
        for (id, vec) in candidates {
            if vec.len() != query.len() || vec.is_empty() {
                out.push((id.clone(), 0.0));
                continue;
            }
            let blob = vec_to_blob(vec);
            let dist: f32 = conn.query_row(
                "SELECT vec_distance_cosine(?1, ?2)",
                params![&q, &blob],
                |row| row.get(0),
            )?;
            let sim = (1.0 - dist).max(0.0);
            out.push((id.clone(), sim));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::cosine;

    #[test]
    fn sqlite_vec_matches_brute_force_on_unit_vectors() {
        let idx = SqliteVecIndex::new().unwrap();
        assert!(idx.version().unwrap().starts_with('v'));
        let q = vec![1.0f32, 0.0];
        let cands = vec![("a".into(), vec![1.0, 0.0]), ("b".into(), vec![0.0, 1.0])];
        let scores = idx.similar(&q, &cands).unwrap();
        assert!((scores[0].1 - cosine(&q, &cands[0].1).max(0.0)).abs() < 1e-5);
        assert!(scores[1].1.abs() < 1e-5);
    }
}
