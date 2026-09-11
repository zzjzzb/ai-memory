import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { createBridge, loadNative, resolveCliPath } from '../src/bridge.js'

const pkgRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const addon = path.join(pkgRoot, 'ai-memory.node')

test('napi HostSession remember + budgeted pack (skipped if addon not built)', {
  skip: !fs.existsSync(addon),
}, () => {
  const native = loadNative()
  assert.ok(native && native.HostSession, 'addon must export HostSession')
  const dbPath = path.join(os.tmpdir(), `ai-memory-live-${process.pid}.db`)
  const bridge = createBridge(
    { dbPath, projectId: 'live', policy: 'chat' },
    { native },
  )
  assert.equal(bridge.kind, 'napi')
  const saved = bridge.dispatch('memory_remember', {
    text: 'live napi prefers linen',
    tier: 'profile',
  })
  assert.equal(saved.ok, true, JSON.stringify(saved))
  const pack = bridge.dispatch('prefetch_within_budget', {
    query: 'linen',
    max_tokens: 256,
  })
  assert.equal(pack.ok, true, JSON.stringify(pack))
  assert.match(pack.data.text, /linen/)
  assert.ok(pack.data.tokens <= 256)
  try {
    fs.unlinkSync(dbPath)
  } catch {
    // ignore
  }
})

test('CLI fallback dispatch uses built binary when present', {
  skip: resolveCliPath('') === 'ai-memory' && !fs.existsSync(path.join(pkgRoot, 'bin', 'ai-memory')),
}, () => {
  const dbPath = path.join(os.tmpdir(), `ai-memory-cli-live-${process.pid}.db`)
  const bridge = createBridge(
    { dbPath, projectId: 'cli-live', policy: 'chat', cliPath: resolveCliPath('') },
    { native: null },
  )
  assert.equal(bridge.kind, 'cli')
  const saved = bridge.dispatch('memory_remember', {
    text: 'cli live note about invoices',
    tier: 'working',
  })
  assert.equal(saved.ok, true, JSON.stringify(saved))
  const pack = bridge.dispatch('prefetch_within_budget', {
    query: 'invoices',
    max_tokens: 128,
  })
  assert.equal(pack.ok, true, JSON.stringify(pack))
  assert.match(pack.data.text, /invoices/)
  try {
    fs.unlinkSync(dbPath)
  } catch {
    // ignore
  }
})
