import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createBridge, createCliBridge, nativeCandidates } from '../src/bridge.js'
import { resolveConfig } from '../src/config.js'

test('nativeCandidates are package-local .node files', () => {
  const paths = nativeCandidates()
  assert.ok(paths.some((p) => p.endsWith('ai-memory.node')))
})

test('createBridge prefers napi HostSession over CLI', () => {
  const calls = []
  const native = {
    HostSession: {
      open(db, project, policy) {
        calls.push(['open', db, project, policy])
        return {
          dispatch(op, json) {
            calls.push(['dispatch', op, json])
            return JSON.stringify({ ok: true, name: op, data: { id: 'm1' }, error: null })
          },
        }
      },
    },
  }
  const bridge = createBridge(
    { ...resolveConfig({}), dbPath: '/tmp/x.db' },
    { native },
  )
  assert.equal(bridge.kind, 'napi')
  const result = bridge.dispatch('memory_remember', { text: 'hi' })
  assert.equal(result.ok, true)
  assert.equal(result.data.id, 'm1')
  assert.equal(calls[0][0], 'open')
  assert.equal(calls[1][0], 'dispatch')
})

test('CLI bridge forwards crate ops and never invents recall', () => {
  const spawned = []
  const spawn = (bin, argv) => {
    spawned.push({ bin, argv })
    return {
      status: 0,
      stdout: JSON.stringify({
        ok: true,
        name: 'prefetch_within_budget',
        data: { text: '## Memory (project: dsh, 1 hits)\n- rust pack', tokens: 12 },
        error: null,
      }),
      stderr: '',
    }
  }
  const bridge = createCliBridge(
    { dbPath: '/tmp/m.db', projectId: 'dsh', policy: 'chat', cliPath: '/opt/ai-memory' },
    spawn,
    '/opt/ai-memory',
  )
  assert.equal(bridge.kind, 'cli')
  const result = bridge.dispatch('prefetch_within_budget', { query: 'q', max_tokens: 64 })
  assert.equal(result.data.text.includes('rust pack'), true)
  assert.equal(spawned[0].bin, '/opt/ai-memory')
  assert.ok(spawned[0].argv.includes('prefetch_within_budget'))
  assert.ok(spawned[0].argv.includes('/tmp/m.db'))
})

test('createBridge uses CLI when napi .node is missing', () => {
  const spawn = (bin, argv) => ({
    status: 0,
    stdout: JSON.stringify({ ok: true, name: argv[4], data: { text: 'cli-pack' }, error: null }),
    stderr: '',
  })
  const bridge = createBridge(
    { ...resolveConfig({}), dbPath: '/tmp/x.db', cliPath: '/opt/ai-memory' },
    { native: null, spawn, cliPath: '/opt/ai-memory' },
  )
  assert.equal(bridge.kind, 'cli')
  const result = bridge.dispatch('memory_recall', { text: 'dark mode' })
  assert.equal(result.ok, true)
  assert.equal(result.data.text, 'cli-pack')
})

test('CLI start failure tells the user to build Rust', () => {
  const spawn = () => ({ error: new Error('ENOENT'), status: 1, stdout: '', stderr: '' })
  const result = createCliBridge(
    { dbPath: 'x', projectId: 'p', policy: 'chat' },
    spawn,
    'missing-bin',
  ).dispatch('memory_recall', { text: 'x' })
  assert.equal(result.ok, false)
  assert.match(result.error, /CLI failed to start/)
})
