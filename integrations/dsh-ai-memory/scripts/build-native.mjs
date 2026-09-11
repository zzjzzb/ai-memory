#!/usr/bin/env node
/**
 * Git install (`dsh plugin add github:…`) does not run `build`, only `prepare`.
 * pnpm ≥10 also requires allowBuilds for this package. See plugin README.
 *
 * Builds:
 *   1. napi cdylib (preferred in-process binding)
 *   2. `ai-memory` CLI (fallback if the addon cannot load)
 *
 * Set DSH_AI_MEMORY_SKIP_NATIVE=1 to skip (JS unit tests only).
 */
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const pluginRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const repoRoot = path.resolve(pluginRoot, '../..')

if (process.env.DSH_AI_MEMORY_SKIP_NATIVE === '1') {
  console.log('[dsh-ai-memory] DSH_AI_MEMORY_SKIP_NATIVE=1 — skipping Rust host build')
  process.exit(0)
}

function run(cmd, args, cwd) {
  const result = spawnSync(cmd, args, { cwd, stdio: 'inherit' })
  return result.status === 0
}

function copyIfExists(from, to) {
  if (!fs.existsSync(from)) return false
  fs.mkdirSync(path.dirname(to), { recursive: true })
  fs.copyFileSync(from, to)
  return true
}

if (!fs.existsSync(path.join(repoRoot, 'Cargo.toml'))) {
  console.warn(
    '[dsh-ai-memory] Cargo workspace not found next to this package. Install from a full ai-memory checkout (not a detached plugin folder).',
  )
  process.exit(0)
}

const cargo = process.env.CARGO || 'cargo'
let builtSomething = false

console.log('[dsh-ai-memory] building ai-memory CLI (Rust source of truth)…')
if (run(cargo, ['build', '--release', '--bin', 'ai-memory'], repoRoot)) {
  const exe = process.platform === 'win32' ? 'ai-memory.exe' : 'ai-memory'
  if (copyIfExists(path.join(repoRoot, 'target', 'release', exe), path.join(pluginRoot, 'bin', exe))) {
    builtSomething = true
    console.log(`[dsh-ai-memory] wrote bin/${exe}`)
  }
} else {
  console.warn('[dsh-ai-memory] CLI build failed')
}

console.log('[dsh-ai-memory] building napi addon…')
if (run(cargo, ['build', '--release', '-p', 'ai-memory-node'], repoRoot)) {
  const libName =
    process.platform === 'win32'
      ? 'ai_memory_node.dll'
      : process.platform === 'darwin'
        ? 'libai_memory_node.dylib'
        : 'libai_memory_node.so'
  const from = path.join(repoRoot, 'target', 'release', libName)
  if (copyIfExists(from, path.join(pluginRoot, 'ai-memory.node'))) {
    builtSomething = true
    console.log('[dsh-ai-memory] wrote ai-memory.node')
  } else {
    console.warn(`[dsh-ai-memory] expected ${from} after napi build`)
  }
} else {
  console.warn('[dsh-ai-memory] napi crate build failed — plugin will use the CLI fallback if present')
}

if (!builtSomething) {
  console.error(
    '[dsh-ai-memory] neither napi addon nor CLI was produced. Install Rust (cargo) and re-run npm run prepare, or set allowBuilds for this package on git install.',
  )
  process.exit(1)
}
