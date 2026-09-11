#!/usr/bin/env node
/**
 * Git install (`dsh plugin add github:zzjzzb/ai-memory`) does not run `build`,
 * only `prepare`. Root package.json `prepare` points here so a github: clone
 * (Cargo.toml at checkout root) can compile. pnpm ≥10 needs allowBuilds for
 * package `dsh-ai-memory`. See docs/INSTALL_DSH.md.
 *
 * Fail-soft: napi is preferred, but a successful CLI build is enough. The
 * plugin then uses the `ai-memory` subprocess. Only fail if neither artifact
 * is produced.
 *
 * Set DSH_AI_MEMORY_SKIP_NATIVE=1 to skip (JS unit tests only).
 */
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const pluginRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const repoRoot = path.resolve(pluginRoot, '../..')

export function napiArtifactNames(platform = process.platform) {
  if (platform === 'win32') {
    return ['ai_memory_node.dll', 'ai_memory_node.node']
  }
  if (platform === 'darwin') {
    return ['libai_memory_node.dylib', 'ai_memory_node.node', 'libai_memory_node.so']
  }
  return ['libai_memory_node.so', 'ai_memory_node.node', 'ai_memory_node.so']
}

/** Exit 0 if either binding exists. napi failure + CLI success is OK. */
export function prepareOutcome({ cliCopied, napiCopied }) {
  if (napiCopied) return { ok: true, binding: 'napi' }
  if (cliCopied) return { ok: true, binding: 'cli' }
  return { ok: false, binding: null }
}

export function cargoBin(env = process.env) {
  return env.CARGO || 'cargo'
}

function run(cmd, args, cwd) {
  const result = spawnSync(cmd, args, { cwd, stdio: 'inherit' })
  return result.status === 0
}

function commandExists(cmd) {
  const probe = spawnSync(cmd, ['--version'], { encoding: 'utf8' })
  return probe.status === 0
}

function copyIfExists(from, to) {
  if (!fs.existsSync(from)) return false
  fs.mkdirSync(path.dirname(to), { recursive: true })
  fs.copyFileSync(from, to)
  return true
}

function copyNapiAddon(releaseDir, destNode) {
  for (const name of napiArtifactNames()) {
    const from = path.join(releaseDir, name)
    if (copyIfExists(from, destNode)) return from
  }
  return null
}

export function runPrepare({
  pluginRoot: pkg = pluginRoot,
  repoRoot: crate = repoRoot,
  env = process.env,
} = {}) {
  if (env.DSH_AI_MEMORY_SKIP_NATIVE === '1') {
    console.log('[dsh-ai-memory] DSH_AI_MEMORY_SKIP_NATIVE=1 — skipping Rust host build')
    return 0
  }

  const cargoToml = path.join(crate, 'Cargo.toml')
  if (!fs.existsSync(cargoToml)) {
    console.error(
      '[dsh-ai-memory] Cargo.toml not found. Install the full repo: github:zzjzzb/ai-memory (not a subdirectory path: spec).',
    )
    return 1
  }

  const cargo = cargoBin(env)
  if (!commandExists(cargo)) {
    console.error(
      `[dsh-ai-memory] \`${cargo} --version\` failed. Install Rust from https://rustup.rs/ (rustc 1.74+) and re-run add, or set DSH_AI_MEMORY_SKIP_NATIVE=1 for JS-only tests.`,
    )
    return 1
  }

  let cliCopied = false
  let napiCopied = false

  console.log('[dsh-ai-memory] building ai-memory CLI (Rust source of truth)…')
  if (run(cargo, ['build', '--release', '--bin', 'ai-memory'], crate)) {
    const exe = process.platform === 'win32' ? 'ai-memory.exe' : 'ai-memory'
    if (copyIfExists(path.join(crate, 'target', 'release', exe), path.join(pkg, 'bin', exe))) {
      cliCopied = true
      console.log(`[dsh-ai-memory] wrote bin/${exe}`)
    }
  } else {
    console.warn('[dsh-ai-memory] CLI build failed — will still try napi')
  }

  console.log('[dsh-ai-memory] building napi addon…')
  if (run(cargo, ['build', '--release', '-p', 'ai-memory-node'], crate)) {
    const dest = path.join(pkg, 'ai-memory.node')
    const from = copyNapiAddon(path.join(crate, 'target', 'release'), dest)
    if (from) {
      napiCopied = true
      console.log(`[dsh-ai-memory] wrote ai-memory.node (from ${path.basename(from)})`)
    } else {
      console.warn(
        `[dsh-ai-memory] napi linked but no addon file found under target/release (${napiArtifactNames().join(', ')})`,
      )
    }
  } else {
    console.warn('[dsh-ai-memory] napi crate build failed — plugin will use the CLI fallback if present')
  }

  const outcome = prepareOutcome({ cliCopied, napiCopied })
  if (!outcome.ok) {
    console.error(
      '[dsh-ai-memory] neither napi addon nor CLI was produced. Install Rust (cargo 1.74+) and re-run npm run prepare, or set allowBuilds for dsh-ai-memory on git install.',
    )
    return 1
  }
  if (outcome.binding === 'cli') {
    console.warn('[dsh-ai-memory] prepare: using CLI fallback (napi addon not built). Runtime still uses the Rust crate, not a JS store.')
  } else {
    console.log('[dsh-ai-memory] prepare: using napi' + (cliCopied ? ' (CLI also built)' : ''))
  }
  return 0
}

function isDirectRun() {
  const self = fileURLToPath(import.meta.url)
  const argv1 = process.argv[1] && path.resolve(process.argv[1])
  return Boolean(argv1) && path.resolve(argv1) === self
}

if (isDirectRun()) {
  process.exit(runPrepare())
}
