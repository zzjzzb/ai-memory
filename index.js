/**
 * DeepSeek Harness bundle entry (`dsh plugin add github:zzjzzb/ai-memory`).
 *
 * Implementation lives in `./integrations/dsh-ai-memory`. Memory storage stays
 * in the Rust crate at this repository root — this file only re-exports the
 * Cordis `apply(ctx)` host.
 */
export {
  name,
  inject,
  Config,
  apply,
  createBridge,
  createConfigSchema,
  lastUserQuery,
  mapToolArgs,
  prefetchArgs,
  resolveConfig,
  resolveDbPath,
  TOOL_SPECS,
} from './integrations/dsh-ai-memory/src/index.js'
