import assert from 'node:assert/strict'
import { test } from 'node:test'
import { napiArtifactNames, prepareOutcome } from '../scripts/build-native.mjs'

test('prepare succeeds when only the CLI was copied (napi fail-soft)', () => {
  assert.deepEqual(prepareOutcome({ cliCopied: true, napiCopied: false }), {
    ok: true,
    binding: 'cli',
  })
})

test('prepare prefers napi when the addon was copied', () => {
  assert.deepEqual(prepareOutcome({ cliCopied: true, napiCopied: true }), {
    ok: true,
    binding: 'napi',
  })
  assert.deepEqual(prepareOutcome({ cliCopied: false, napiCopied: true }), {
    ok: true,
    binding: 'napi',
  })
})

test('prepare fails only when neither binding exists', () => {
  assert.deepEqual(prepareOutcome({ cliCopied: false, napiCopied: false }), {
    ok: false,
    binding: null,
  })
})

test('napi artifact names include the platform cdylib', () => {
  assert.ok(napiArtifactNames('linux').includes('libai_memory_node.so'))
  assert.ok(napiArtifactNames('darwin').includes('libai_memory_node.dylib'))
  assert.ok(napiArtifactNames('win32').includes('ai_memory_node.dll'))
})
