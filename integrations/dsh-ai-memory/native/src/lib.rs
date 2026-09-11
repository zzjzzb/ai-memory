//! napi-rs binding. All memory logic lives in `ai-memory` (Rust + SQLite).

use ai_memory::HostSession as RustHost;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json::Value;

#[napi]
pub struct HostSession {
    inner: RustHost,
}

#[napi]
impl HostSession {
    #[napi(factory)]
    pub fn open(db_path: String, project_id: String, policy: Option<String>) -> Result<Self> {
        let policy = policy.as_deref().unwrap_or("chat");
        let inner = RustHost::open(db_path, project_id, policy)
            .map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self { inner })
    }

    #[napi(factory)]
    pub fn open_in_memory(project_id: String, policy: Option<String>) -> Result<Self> {
        let policy = policy.as_deref().unwrap_or("chat");
        let inner = RustHost::open_in_memory(project_id, policy)
            .map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self { inner })
    }

    #[napi]
    pub fn project_id(&self) -> String {
        self.inner.project_id().to_string()
    }

    /// `op` + JSON args → `{ok, name, data, error}` JSON string.
    #[napi]
    pub fn dispatch(&self, op: String, args_json: Option<String>) -> Result<String> {
        let args: Value = match args_json.as_deref() {
            None | Some("") => Value::Object(Default::default()),
            Some(raw) => serde_json::from_str(raw)
                .map_err(|e| Error::from_reason(format!("invalid JSON args: {e}")))?,
        };
        Ok(self.inner.dispatch(&op, args).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_handle_roundtrip() {
        let host = HostSession::open_in_memory("napi".into(), Some("chat".into())).unwrap();
        let saved = host
            .dispatch(
                "memory_remember".into(),
                Some(r#"{"text":"napi smoke note","tier":"working"}"#.into()),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(v["ok"], true, "{v}");
        let pack = host
            .dispatch(
                "prefetch_within_budget".into(),
                Some(r#"{"query":"napi smoke","max_tokens":128}"#.into()),
            )
            .unwrap();
        let pack: serde_json::Value = serde_json::from_str(&pack).unwrap();
        assert!(pack["data"]["text"]
            .as_str()
            .unwrap()
            .contains("napi smoke note"));
    }
}
