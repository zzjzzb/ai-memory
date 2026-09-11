import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  HARNESS_TOOL_NAMES,
  mapToolArgs,
  prefetchArgs,
  TOOL_COMPACT,
  TOOL_CONSOLIDATE,
  TOOL_FORGET,
  TOOL_PIN,
  TOOL_RECALL,
  TOOL_REMEMBER,
  TOOL_SPECS,
  toolOutputText,
} from '../src/tools.js'
import { lastUserQuery, textFromUserMessage } from '../src/prompt.js'

test('tool specs use crate harness names', () => {
  const names = TOOL_SPECS.map((s) => s.name)
  for (const name of HARNESS_TOOL_NAMES) {
    assert.ok(names.includes(name), name)
  }
  assert.ok(names.includes(TOOL_COMPACT))
})

test('mapToolArgs remember / recall / pin', () => {
  assert.deepEqual(
    mapToolArgs(TOOL_REMEMBER, { text: 'hi', tier: 'profile', extra: true }),
    { text: 'hi', tier: 'profile' },
  )
  assert.deepEqual(mapToolArgs(TOOL_RECALL, { text: 'q', limit: 3 }), { text: 'q', limit: 3 })
  assert.deepEqual(mapToolArgs(TOOL_PIN, { memory_id: 'm1' }), { memory_id: 'm1' })
  assert.deepEqual(mapToolArgs(TOOL_FORGET, { memory_id: 'm1' }), { memory_id: 'm1' })
  assert.deepEqual(mapToolArgs(TOOL_CONSOLIDATE, { ignored: 1 }), {})
  assert.deepEqual(mapToolArgs(TOOL_COMPACT, {}), {})
})

test('mapToolArgs rejects unknown tools', () => {
  assert.throws(() => mapToolArgs('memory_extract_llm', {}), /unknown memory tool/)
})

test('prefetchArgs maps budget field expected by HostSession', () => {
  assert.deepEqual(prefetchArgs('sidebar', 2048), { query: 'sidebar', max_tokens: 2048 })
  assert.deepEqual(prefetchArgs(null, 8), { query: '', max_tokens: 8 })
})

test('toolOutputText prefers recall context pack', () => {
  assert.equal(
    toolOutputText({
      ok: true,
      name: TOOL_RECALL,
      data: { hits: [], context: '## Memory\n- note' },
    }),
    '## Memory\n- note',
  )
  assert.match(toolOutputText({ ok: false, error: 'missing string field `text`' }), /missing string/)
})

test('lastUserQuery reads session user/message events', () => {
  const ctx = {
    agent: {
      session: {
        events: [
          { type: 'turn/start' },
          { type: 'user/message', data: { content: [{ type: 'text', text: 'fix the sidebar' }] } },
        ],
      },
    },
  }
  assert.equal(lastUserQuery(ctx, 'fallback'), 'fix the sidebar')
  assert.equal(lastUserQuery({}, 'fallback'), 'fallback')
  assert.equal(textFromUserMessage({ content: 'plain' }), 'plain')
})
