import assert from 'node:assert/strict'
import { test } from 'node:test'
import { resolveConfig, resolveDbPath } from '../src/config.js'

test('resolveConfig fills defaults', () => {
  const cfg = resolveConfig({})
  assert.equal(cfg.projectId, 'dsh')
  assert.equal(cfg.tokenBudget, 8192)
  assert.equal(cfg.policy, 'chat')
  assert.equal(cfg.prefetchEnabled, true)
  assert.equal(cfg.sectionOrder, 40)
})

test('resolveConfig rejects non-positive budget', () => {
  assert.throws(() => resolveConfig({ tokenBudget: 0 }), /tokenBudget/)
  assert.throws(() => resolveConfig({ tokenBudget: -4 }), /tokenBudget/)
  assert.throws(() => resolveConfig({ tokenBudget: 'nope' }), /tokenBudget/)
})

test('resolveConfig rejects unknown policy and empty project', () => {
  assert.throws(() => resolveConfig({ policy: 'auto-llm' }), /policy/)
  assert.throws(() => resolveConfig({ projectId: '  ' }), /projectId/)
})

test('resolveConfig floors token budget', () => {
  assert.equal(resolveConfig({ tokenBudget: 4096.9 }).tokenBudget, 4096)
})

test('resolveDbPath expands ~ and env override', () => {
  const prev = process.env.AI_MEMORY_DB
  delete process.env.AI_MEMORY_DB
  const home = resolveDbPath('~/ai-memory-test.db')
  assert.match(home, /ai-memory-test\.db$/)
  process.env.AI_MEMORY_DB = '/tmp/from-env.db'
  assert.equal(resolveDbPath(''), '/tmp/from-env.db')
  if (prev === undefined) delete process.env.AI_MEMORY_DB
  else process.env.AI_MEMORY_DB = prev
})
