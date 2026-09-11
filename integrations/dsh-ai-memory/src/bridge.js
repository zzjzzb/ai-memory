import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const here = path.dirname(fileURLToPath(import.meta.url))
const pkgRoot = path.resolve(here, '..')

export function nativeCandidates() {
  return [
    path.join(pkgRoot, 'ai-memory.node'),
    path.join(pkgRoot, 'ai-memory.linux-x64-gnu.node'),
    path.join(pkgRoot, 'ai-memory.darwin-arm64.node'),
    path.join(pkgRoot, 'ai-memory.darwin-x64.node'),
    path.join(pkgRoot, 'ai-memory.win32-x64-msvc.node'),
    path.join(pkgRoot, 'native', 'index.node'),
  ]
}

export function loadNative(requireFn = require) {
  for (const candidate of nativeCandidates()) {
    if (!fs.existsSync(candidate)) continue
    try {
      return requireFn(candidate)
    } catch {
      // try the next path
    }
  }
  return null
}

export function repoRootFromPlugin() {
  return path.resolve(pkgRoot, '../..')
}

export function resolveCliPath(explicit) {
  if (explicit) return explicit
  if (process.env.AI_MEMORY_CLI) return process.env.AI_MEMORY_CLI
  const bundled = path.join(pkgRoot, 'bin', process.platform === 'win32' ? 'ai-memory.exe' : 'ai-memory')
  if (fs.existsSync(bundled)) return bundled
  const root = repoRootFromPlugin()
  const release = path.join(root, 'target', 'release', 'ai-memory')
  const debug = path.join(root, 'target', 'debug', 'ai-memory')
  if (fs.existsSync(release)) return release
  if (fs.existsSync(debug)) return debug
  return 'ai-memory'
}

function parseEnvelope(text, fallbackName) {
  const trimmed = String(text || '').trim()
  try {
    return JSON.parse(trimmed)
  } catch {
    return {
      ok: false,
      name: fallbackName,
      data: null,
      error: trimmed || 'empty host response',
    }
  }
}

export function createNapiBridge(native, config) {
  const Session = native.HostSession
  const session = Session.open(config.dbPath, config.projectId, config.policy)
  return {
    kind: 'napi',
    dispatch(op, args) {
      const raw = session.dispatch(op, JSON.stringify(args ?? {}))
      return parseEnvelope(raw, op)
    },
    close() {},
  }
}

export function createCliBridge(config, spawn = spawnSync, cliPath = resolveCliPath(config.cliPath)) {
  return {
    kind: 'cli',
    dispatch(op, args) {
      const result = spawn(
        cliPath,
        [
          '--db',
          config.dbPath,
          '--project',
          config.projectId,
          '--policy',
          config.policy,
          op,
          JSON.stringify(args ?? {}),
        ],
        { encoding: 'utf8' },
      )
      if (result.error) {
        return {
          ok: false,
          name: op,
          data: null,
          error: `ai-memory CLI failed to start (${cliPath}): ${result.error.message}. Build the napi addon (npm run prepare) or cargo build --bin ai-memory.`,
        }
      }
      const envelope = parseEnvelope(result.stdout, op)
      if (result.status !== 0 && envelope.ok !== false) {
        const err = (result.stderr || '').trim()
        envelope.ok = false
        envelope.error = envelope.error || err || `cli exited ${result.status}`
      }
      return envelope
    },
    close() {},
  }
}

/**
 * Prefer the napi addon (in-process Rust). Fall back to the `ai-memory` CLI.
 * Never reimplements recall / pack / consolidate in JavaScript.
 */
export function createBridge(config, deps = {}) {
  const native = deps.native !== undefined ? deps.native : loadNative(deps.requireFn)
  if (native && native.HostSession) {
    return createNapiBridge(native, config)
  }
  return createCliBridge(config, deps.spawn, deps.cliPath ?? resolveCliPath(config.cliPath))
}
