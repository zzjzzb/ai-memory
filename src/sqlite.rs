use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::embed_cache::CachedEmbedder;
use crate::embedder::{Embedder, HashEmbedder};
use crate::error::{Error, Result};
use crate::heuristic::infer_tier;
use crate::policy::{hybrid_score, keyword_score, memory_expired, recency_score, MemoryPolicy};
use crate::store::{MemoryStore, ProjectHandle};
use crate::types::{
    ConsolidateReport, Memory, MemoryListFilter, Project, RecallHit, RecallQuery, RememberRequest,
    Tier,
};
use crate::util::{blob_to_vec, ms_to_time, new_id, now_ms, time_to_ms, tokenize, vec_to_blob};
use crate::vector::{BruteForceCosine, VectorIndex};

/// Local SQLite-backed store. One file (or in-memory DB) serves every project.
#[derive(Clone)]
pub struct SqliteStore {
    inner: Arc<Inner>,
}

struct Inner {
    conn: Mutex<Connection>,
    embedder: Arc<dyn Embedder>,
    vectors: Arc<dyn VectorIndex>,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStore").finish_non_exhaustive()
    }
}

/// Builder for [`SqliteStore`].
#[derive(Default)]
pub struct SqliteStoreBuilder {
    path: Option<PathBuf>,
    embedder: Option<Arc<dyn Embedder>>,
    vectors: Option<Arc<dyn VectorIndex>>,
}

impl SqliteStoreBuilder {
    pub fn path(mut self, path: impl AsRef<Path>) -> Self {
        self.path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn in_memory(mut self) -> Self {
        self.path = None;
        self
    }

    pub fn embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    pub fn vector_index(mut self, vectors: Arc<dyn VectorIndex>) -> Self {
        self.vectors = Some(vectors);
        self
    }

    pub fn build(self) -> Result<SqliteStore> {
        let conn = match &self.path {
            Some(path) => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent)?;
                    }
                }
                Connection::open(path)?
            }
            None => Connection::open_in_memory()?,
        };
        apply_runtime_pragmas(&conn, self.path.is_some())?;
        init_schema(&conn)?;
        let raw = self
            .embedder
            .unwrap_or_else(|| Arc::new(HashEmbedder::default()));
        Ok(SqliteStore {
            inner: Arc::new(Inner {
                conn: Mutex::new(conn),
                embedder: Arc::new(CachedEmbedder::wrap(raw)),
                vectors: self.vectors.unwrap_or_else(|| Arc::new(BruteForceCosine)),
            }),
        })
    }
}

impl SqliteStore {
    pub fn builder() -> SqliteStoreBuilder {
        SqliteStoreBuilder::default()
    }

    /// Open or create a file-backed store. Same PRAGMAs as [`crate::open`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::builder().path(path).build()
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::builder().in_memory().build()
    }

    pub fn open_with_embedder(path: impl AsRef<Path>, embedder: Arc<dyn Embedder>) -> Result<Self> {
        Self::builder().path(path).embedder(embedder).build()
    }

    pub fn open_in_memory_with_embedder(embedder: Arc<dyn Embedder>) -> Result<Self> {
        Self::builder().in_memory().embedder(embedder).build()
    }

    /// PRAGMAs applied automatically by [`SqliteStore::open`] / [`open`](crate::open).
    pub fn applied_pragmas(&self) -> Result<AppliedPragmas> {
        let conn = self.lock()?;
        Ok(AppliedPragmas {
            foreign_keys: pragma_i64(&conn, "foreign_keys")? != 0,
            journal_mode: pragma_text(&conn, "journal_mode")?,
            synchronous: pragma_i64(&conn, "synchronous")?,
            temp_store: pragma_i64(&conn, "temp_store")?,
            cache_size: pragma_i64(&conn, "cache_size")?,
            busy_timeout_ms: pragma_i64(&conn, "busy_timeout")?,
        })
    }

    /// Embedder used for `remember` / `recall` (process LRU wrapper around the injected impl).
    pub fn embedder(&self) -> Arc<dyn Embedder> {
        Arc::clone(&self.inner.embedder)
    }

    /// Scoped handle; all subsequent calls stay inside this project.
    pub fn project(&self, id: &str) -> Result<ProjectHandle<Self>> {
        if self.get_project(id)?.is_none() {
            return Err(Error::ProjectNotFound(id.to_string()));
        }
        Ok(ProjectHandle::new(self.clone(), id.to_string()))
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.inner.conn.lock().map_err(|_| Error::Poisoned)
    }

    fn require_project(conn: &Connection, project_id: &str) -> Result<()> {
        let mut stmt = conn.prepare_cached("SELECT 1 FROM projects WHERE id = ?1")?;
        let exists: Option<i64> = stmt
            .query_row(params![project_id], |row| row.get(0))
            .optional()?;
        if exists.is_none() {
            Err(Error::ProjectNotFound(project_id.to_string()))
        } else {
            Ok(())
        }
    }

    fn load_policy(conn: &Connection, project_id: &str) -> Result<MemoryPolicy> {
        let mut stmt = conn.prepare_cached("SELECT policy_json FROM projects WHERE id = ?1")?;
        let json: String = stmt
            .query_row(params![project_id], |row| row.get(0))
            .optional()?
            .ok_or_else(|| Error::ProjectNotFound(project_id.to_string()))?;
        Ok(serde_json::from_str(&json)?)
    }
}

impl MemoryStore for SqliteStore {
    fn create_project(&self, id: &str, policy: MemoryPolicy) -> Result<Project> {
        if id.trim().is_empty() {
            return Err(Error::InvalidPolicy("project id must not be empty".into()));
        }
        policy.validate()?;
        let conn = self.lock()?;
        let exists: Option<i64> = conn
            .query_row("SELECT 1 FROM projects WHERE id = ?1", params![id], |row| {
                row.get(0)
            })
            .optional()?;
        if exists.is_some() {
            return Err(Error::ProjectExists(id.to_string()));
        }
        let created = now_ms();
        let json = serde_json::to_string(&policy)?;
        conn.execute(
            "INSERT INTO projects (id, policy_json, created_at) VALUES (?1, ?2, ?3)",
            params![id, json, created],
        )?;
        Ok(Project {
            id: id.to_string(),
            policy,
            created_at: ms_to_time(created),
        })
    }

    fn get_project(&self, id: &str) -> Result<Option<Project>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT id, policy_json, created_at FROM projects WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((pid, json, created)) => Ok(Some(Project {
                id: pid,
                policy: serde_json::from_str(&json)?,
                created_at: ms_to_time(created),
            })),
        }
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        let conn = self.lock()?;
        let mut stmt =
            conn.prepare("SELECT id, policy_json, created_at FROM projects ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, json, created) = row?;
            out.push(Project {
                id,
                policy: serde_json::from_str(&json)?,
                created_at: ms_to_time(created),
            });
        }
        Ok(out)
    }

    fn set_policy(&self, project_id: &str, policy: MemoryPolicy) -> Result<()> {
        policy.validate()?;
        let conn = self.lock()?;
        Self::require_project(&conn, project_id)?;
        let json = serde_json::to_string(&policy)?;
        let n = conn.execute(
            "UPDATE projects SET policy_json = ?1 WHERE id = ?2",
            params![json, project_id],
        )?;
        if n == 0 {
            Err(Error::ProjectNotFound(project_id.to_string()))
        } else {
            Ok(())
        }
    }

    fn policy(&self, project_id: &str) -> Result<MemoryPolicy> {
        let conn = self.lock()?;
        Self::load_policy(&conn, project_id)
    }

    fn delete_project(&self, project_id: &str) -> Result<()> {
        let conn = self.lock()?;
        Self::require_project(&conn, project_id)?;
        conn.execute("DELETE FROM projects WHERE id = ?1", params![project_id])?;
        Ok(())
    }

    fn remember(&self, project_id: &str, req: RememberRequest) -> Result<Memory> {
        let mut out = self.remember_many(project_id, vec![req])?;
        out.pop().ok_or(Error::EmptyText)
    }

    fn remember_many(&self, project_id: &str, reqs: Vec<RememberRequest>) -> Result<Vec<Memory>> {
        if reqs.is_empty() {
            return Ok(Vec::new());
        }

        struct Prepared {
            text: String,
            tier: Tier,
            metadata: Option<serde_json::Value>,
            embedding: Vec<f32>,
        }

        let mut prepared = Vec::with_capacity(reqs.len());
        for req in reqs {
            let text = req.text.trim().to_string();
            if text.is_empty() {
                return Err(Error::EmptyText);
            }
            let tier = req.tier.unwrap_or_else(|| infer_tier(&text));
            let embedding = self.inner.embedder.embed(&text)?;
            prepared.push(Prepared {
                text,
                tier,
                metadata: req.metadata,
                embedding,
            });
        }

        let mut conn = self.lock()?;
        Self::require_project(&conn, project_id)?;
        let tx = conn.transaction()?;
        let now = now_ms();
        let mut out = Vec::with_capacity(prepared.len());
        {
            let mut ins_mem = tx.prepare_cached(
                "INSERT INTO memories (
                    id, project_id, tier, text, metadata_json,
                    created_at, updated_at, last_accessed_at, access_count, pinned
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 0, 0)",
            )?;
            let mut ins_emb = tx.prepare_cached(
                "INSERT INTO embeddings (memory_id, dim, vector) VALUES (?1, ?2, ?3)",
            )?;
            for item in prepared {
                let id = new_id();
                let metadata_json = match &item.metadata {
                    Some(v) => Some(serde_json::to_string(v)?),
                    None => None,
                };
                ins_mem.execute(params![
                    id,
                    project_id,
                    item.tier.as_str(),
                    item.text,
                    metadata_json,
                    now,
                    now
                ])?;
                ins_emb.execute(params![
                    id,
                    item.embedding.len() as i64,
                    vec_to_blob(&item.embedding)
                ])?;
                out.push(Memory {
                    id,
                    project_id: project_id.to_string(),
                    tier: item.tier,
                    text: item.text,
                    metadata: item.metadata,
                    created_at: ms_to_time(now),
                    updated_at: ms_to_time(now),
                    last_accessed_at: None,
                    access_count: 0,
                    pinned: false,
                });
            }
        }
        tx.commit()?;
        Ok(out)
    }

    fn get(&self, project_id: &str, memory_id: &str) -> Result<Option<Memory>> {
        let conn = self.lock()?;
        Self::require_project(&conn, project_id)?;
        let mut stmt = conn.prepare_cached(
            "SELECT id, project_id, tier, text, metadata_json, created_at, updated_at,
                    last_accessed_at, access_count, pinned
             FROM memories WHERE project_id = ?1 AND id = ?2",
        )?;
        let mem = stmt
            .query_row(params![project_id, memory_id], map_memory)
            .optional()?;
        Ok(mem)
    }

    fn list_memories(&self, project_id: &str) -> Result<Vec<Memory>> {
        self.list_memories_filtered(project_id, MemoryListFilter::default())
    }

    fn list_memories_filtered(
        &self,
        project_id: &str,
        filter: MemoryListFilter,
    ) -> Result<Vec<Memory>> {
        let conn = self.lock()?;
        let policy = Self::load_policy(&conn, project_id)?;
        let mut sql = String::from(
            "SELECT id, project_id, tier, text, metadata_json, created_at, updated_at,
                    last_accessed_at, access_count, pinned
             FROM memories WHERE project_id = ?",
        );
        let mut bind: Vec<rusqlite::types::Value> = vec![project_id.to_string().into()];
        if let Some(since) = filter.since {
            sql.push_str(" AND created_at >= ?");
            bind.push(time_to_ms(since).into());
        }
        if let Some(until) = filter.until {
            sql.push_str(" AND created_at <= ?");
            bind.push(time_to_ms(until).into());
        }
        if let Some(tiers) = &filter.tiers {
            if tiers.is_empty() {
                return Ok(Vec::new());
            }
            sql.push_str(" AND tier IN (");
            for (i, t) in tiers.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                sql.push('?');
                bind.push(t.as_str().to_string().into());
            }
            sql.push(')');
        }
        if let Some(pinned) = filter.pinned {
            sql.push_str(" AND pinned = ?");
            bind.push(if pinned { 1i64 } else { 0i64 }.into());
        }
        sql.push_str(" ORDER BY created_at ASC");

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(bind.iter()), map_memory)?;
        let now = SystemTime::now();
        let mut out = Vec::new();
        for row in rows {
            let memory = row?;
            if !filter.include_expired
                && memory_expired(&policy, memory.pinned, memory.tier, memory.created_at, now)
            {
                continue;
            }
            out.push(memory);
            if let Some(limit) = filter.limit {
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    fn pin(&self, project_id: &str, memory_id: &str) -> Result<()> {
        let conn = self.lock()?;
        set_pinned(&conn, project_id, memory_id, true)
    }

    fn unpin(&self, project_id: &str, memory_id: &str) -> Result<()> {
        let conn = self.lock()?;
        set_pinned(&conn, project_id, memory_id, false)
    }

    fn forget(&self, project_id: &str, memory_id: &str) -> Result<()> {
        let conn = self.lock()?;
        Self::require_project(&conn, project_id)?;
        // embeddings cascade
        let n = conn.execute(
            "DELETE FROM memories WHERE project_id = ?1 AND id = ?2",
            params![project_id, memory_id],
        )?;
        if n == 0 {
            Err(Error::MemoryNotFound {
                project_id: project_id.to_string(),
                memory_id: memory_id.to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn recall(&self, project_id: &str, query: RecallQuery) -> Result<Vec<RecallHit>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        let q_embed = self.inner.embedder.embed(&query.text)?;
        let q_tokens = tokenize(&query.text);

        let conn = self.lock()?;
        let policy = Self::load_policy(&conn, project_id)?;

        let mut sql = String::from(
            "SELECT m.id, m.project_id, m.tier, m.text, m.metadata_json, m.created_at,
                    m.updated_at, m.last_accessed_at, m.access_count, m.pinned
             FROM memories m
             WHERE m.project_id = ?",
        );
        let mut bind: Vec<rusqlite::types::Value> = vec![project_id.to_string().into()];
        if let Some(since) = query.since {
            sql.push_str(" AND m.created_at >= ?");
            bind.push(time_to_ms(since).into());
        }
        if let Some(until) = query.until {
            sql.push_str(" AND m.created_at <= ?");
            bind.push(time_to_ms(until).into());
        }
        if let Some(tiers) = &query.tiers {
            if tiers.is_empty() {
                return Ok(Vec::new());
            }
            sql.push_str(" AND m.tier IN (");
            for (i, t) in tiers.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                sql.push('?');
                bind.push(t.as_str().to_string().into());
            }
            sql.push(')');
        }
        sql.push_str(" ORDER BY m.pinned DESC, m.created_at DESC");
        let scan = policy.recall.scan_limit;
        if scan > 0 {
            sql.push_str(" LIMIT ?");
            bind.push((scan as i64).into());
        }

        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(bind.iter()), map_memory)?;

        let now = SystemTime::now();
        let mut live: Vec<Memory> = Vec::new();
        for row in rows {
            let memory = row?;
            if memory_expired(&policy, memory.pinned, memory.tier, memory.created_at, now) {
                continue;
            }
            live.push(memory);
        }
        drop(stmt);

        // Cheap keyword+recency prune *before* loading embedding blobs / VectorIndex.
        let prune = query
            .candidate_limit
            .unwrap_or(policy.recall.candidate_prune);
        if prune > 0 && live.len() > prune {
            live.sort_by(|a, b| {
                // Pinned rows stay in the candidate set (TTL already applied).
                b.pinned.cmp(&a.pinned).then_with(|| {
                    let cheap = |m: &Memory| {
                        let age = now
                            .duration_since(m.created_at)
                            .unwrap_or(Duration::from_secs(0));
                        let kw = keyword_score(&q_tokens, &tokenize(&m.text));
                        let ts = recency_score(age, policy.recall.time_half_life);
                        kw + ts
                    };
                    cheap(b)
                        .partial_cmp(&cheap(a))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });
            live.truncate(prune);
        }

        // Stable, insertion-like order for VectorIndex (hybrid ranking uses scores, not this order).
        live.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let mut candidates = Vec::with_capacity(live.len());
        let blobs = load_embedding_blobs(&conn, live.iter().map(|m| m.id.as_str()))?;
        for memory in &live {
            let vec = blobs.get(&memory.id).cloned().unwrap_or_default();
            candidates.push((memory.id.clone(), vec));
        }

        let sims = self.inner.vectors.similar(&q_embed, &candidates)?;
        let mut sim_map = std::collections::HashMap::new();
        for (id, s) in sims {
            sim_map.insert(id, s);
        }

        let now = SystemTime::now();
        let mut hits: Vec<RecallHit> = live
            .into_iter()
            .map(|memory| {
                let age = now
                    .duration_since(memory.created_at)
                    .unwrap_or(Duration::from_secs(0));
                let kw = keyword_score(&q_tokens, &tokenize(&memory.text));
                let vs = sim_map.get(&memory.id).copied().unwrap_or(0.0);
                let (score, time_score, keyword_score, vector_score) =
                    hybrid_score(&policy.recall, age, kw, vs);
                RecallHit {
                    memory,
                    score,
                    time_score,
                    keyword_score,
                    vector_score,
                }
            })
            .filter(|h| query.min_score.map(|m| h.score >= m).unwrap_or(true))
            .collect();

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory.created_at.cmp(&a.memory.created_at))
        });
        hits.truncate(query.limit);

        let now_ms_val = now_ms();
        {
            let mut touch = conn.prepare_cached(
                "UPDATE memories
                 SET access_count = access_count + 1,
                     last_accessed_at = ?1,
                     updated_at = ?1
                 WHERE project_id = ?2 AND id = ?3",
            )?;
            for hit in &hits {
                touch.execute(params![now_ms_val, project_id, hit.memory.id])?;
            }
        }

        Ok(hits)
    }

    fn consolidate(&self, project_id: &str) -> Result<ConsolidateReport> {
        let conn = self.lock()?;
        let policy = Self::load_policy(&conn, project_id)?;
        let now = now_ms();
        let mut stmt = conn.prepare(
            "SELECT id, tier, created_at, access_count, pinned
             FROM memories WHERE project_id = ?1",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(MemState {
                id: row.get(0)?,
                tier: row.get(1)?,
                created_at: row.get(2)?,
                access_count: row.get::<_, i64>(3)? as u32,
                pinned: row.get::<_, i64>(4)? != 0,
            })
        })?;

        let mut expired = Vec::new();
        let mut to_episodic = Vec::new();
        let mut to_profile = Vec::new();

        for row in rows {
            let m = row?;
            let tier = match Tier::parse(&m.tier) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let age_ms = (now - m.created_at).max(0) as u64;
            let age = Duration::from_millis(age_ms);

            let skip_expiry = m.pinned && policy.promote.pinned_skip_expiry;
            if !skip_expiry {
                if let Some(ttl) = policy.retention.ttl(tier) {
                    if age >= ttl {
                        expired.push(m.id);
                        continue;
                    }
                }
            }

            match tier {
                Tier::Working if age >= policy.promote.working_to_episodic_after => {
                    to_episodic.push(m.id);
                }
                Tier::Episodic
                    if age >= policy.promote.episodic_to_profile_after
                        && m.access_count >= policy.promote.episodic_to_profile_min_accesses =>
                {
                    to_profile.push(m.id);
                }
                _ => {}
            }
        }
        drop(stmt);

        let mut report = ConsolidateReport::default();
        for id in &expired {
            conn.execute(
                "DELETE FROM memories WHERE project_id = ?1 AND id = ?2",
                params![project_id, id],
            )?;
            report.expired += 1;
        }
        for id in &to_episodic {
            conn.execute(
                "UPDATE memories SET tier = 'episodic', updated_at = ?1
                 WHERE project_id = ?2 AND id = ?3",
                params![now, project_id, id],
            )?;
            report.promoted_to_episodic += 1;
        }
        for id in &to_profile {
            conn.execute(
                "UPDATE memories SET tier = 'profile', updated_at = ?1
                 WHERE project_id = ?2 AND id = ?3",
                params![now, project_id, id],
            )?;
            report.promoted_to_profile += 1;
        }
        Ok(report)
    }
}

struct MemState {
    id: String,
    tier: String,
    created_at: i64,
    access_count: u32,
    pinned: bool,
}

fn set_pinned(conn: &Connection, project_id: &str, memory_id: &str, pinned: bool) -> Result<()> {
    SqliteStore::require_project(conn, project_id)?;
    let n = conn.execute(
        "UPDATE memories SET pinned = ?1, updated_at = ?2
         WHERE project_id = ?3 AND id = ?4",
        params![if pinned { 1 } else { 0 }, now_ms(), project_id, memory_id],
    )?;
    if n == 0 {
        Err(Error::MemoryNotFound {
            project_id: project_id.to_string(),
            memory_id: memory_id.to_string(),
        })
    } else {
        Ok(())
    }
}

fn map_memory(row: &Row<'_>) -> rusqlite::Result<Memory> {
    let tier_s: String = row.get(2)?;
    let tier = Tier::parse(&tier_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let metadata_json: Option<String> = row.get(4)?;
    let metadata = match metadata_json {
        Some(s) if !s.is_empty() => serde_json::from_str(&s).ok(),
        _ => None,
    };
    Ok(Memory {
        id: row.get(0)?,
        project_id: row.get(1)?,
        tier,
        text: row.get(3)?,
        metadata,
        created_at: ms_to_time(row.get(5)?),
        updated_at: ms_to_time(row.get(6)?),
        last_accessed_at: row.get::<_, Option<i64>>(7)?.map(ms_to_time),
        access_count: row.get::<_, i64>(8)? as u32,
        pinned: row.get::<_, i64>(9)? != 0,
    })
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '1');

        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            policy_json TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            tier TEXT NOT NULL CHECK (tier IN ('working', 'episodic', 'profile')),
            text TEXT NOT NULL,
            metadata_json TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            last_accessed_at INTEGER,
            access_count INTEGER NOT NULL DEFAULT 0,
            pinned INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_memories_project ON memories(project_id);
        CREATE INDEX IF NOT EXISTS idx_memories_project_tier ON memories(project_id, tier);
        CREATE INDEX IF NOT EXISTS idx_memories_project_created ON memories(project_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_memories_project_pinned_created
            ON memories(project_id, pinned, created_at);

        CREATE TABLE IF NOT EXISTS embeddings (
            memory_id TEXT PRIMARY KEY,
            dim INTEGER NOT NULL,
            vector BLOB NOT NULL,
            FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
        );
        "#,
    )?;
    Ok(())
}

/// Values `open()` / `open_in_memory()` apply without extra knobs.
#[derive(Clone, Debug, PartialEq)]
pub struct AppliedPragmas {
    pub foreign_keys: bool,
    pub journal_mode: String,
    pub synchronous: i64,
    pub temp_store: i64,
    pub cache_size: i64,
    pub busy_timeout_ms: i64,
}

/// ~16 MiB page cache (`PRAGMA cache_size` negative = KiB).
const SQLITE_CACHE_SIZE: i64 = -16_384;
const SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;
const SQLITE_STMT_CACHE: usize = 64;

fn apply_runtime_pragmas(conn: &Connection, file_backed: bool) -> Result<()> {
    conn.execute_batch(&format!(
        "PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;
         PRAGMA cache_size = {SQLITE_CACHE_SIZE};"
    ))?;
    conn.busy_timeout(Duration::from_millis(SQLITE_BUSY_TIMEOUT_MS))?;
    conn.set_prepared_statement_cache_capacity(SQLITE_STMT_CACHE);
    if file_backed {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
    }
    Ok(())
}

fn pragma_i64(conn: &Connection, name: &str) -> Result<i64> {
    let sql = format!("PRAGMA {name}");
    Ok(conn.query_row(&sql, [], |row| row.get(0))?)
}

fn pragma_text(conn: &Connection, name: &str) -> Result<String> {
    let sql = format!("PRAGMA {name}");
    Ok(conn.query_row(&sql, [], |row| row.get(0))?)
}

fn load_embedding_blobs<'a>(
    conn: &Connection,
    ids: impl IntoIterator<Item = &'a str>,
) -> Result<HashMap<String, Vec<f32>>> {
    let ids: Vec<&str> = ids.into_iter().collect();
    let mut out = HashMap::with_capacity(ids.len());
    if ids.is_empty() {
        return Ok(out);
    }
    let mut sql = String::from("SELECT memory_id, vector FROM embeddings WHERE memory_id IN (");
    for (i, _) in ids.iter().enumerate() {
        if i > 0 {
            sql.push(',');
        }
        sql.push('?');
    }
    sql.push(')');
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| {
        let id: String = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        Ok((id, blob))
    })?;
    for row in rows {
        let (id, blob) = row?;
        if let Some(v) = blob_to_vec(&blob) {
            out.insert(id, v);
        }
    }
    Ok(out)
}
