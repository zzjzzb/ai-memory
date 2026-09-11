import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { createRequire } from 'node:module'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const require = createRequire(import.meta.url)
const pkg = require('../package.json')

test('package.json declares an installable dsh.bundle.patch', () => {
  assert.equal(pkg.name, 'dsh-ai-memory')
  assert.equal(pkg.dsh.bundle.patch, './cordis.patch.yml')
  assert.equal(pkg.type, 'module')
  assert.equal(pkg.scripts.prepare, 'node scripts/build-native.mjs')
  const patchPath = path.join(root, 'cordis.patch.yml')
  assert.ok(fs.existsSync(patchPath))
  const patch = fs.readFileSync(patchPath, 'utf8')
  assert.match(patch, /name:\s*dsh-ai-memory/)
  assert.match(patch, /id:\s*dsh-ai-memory/)
})

test('apply(ctx) registers crate tool names without a full dsh runtime', async () => {
  const { apply } = await import('../src/index.js')
  const registered = []
  const sections = []
  const ctx = {
    tools: {
      register(tool) {
        registered.push(tool)
      },
    },
    systemPrompt: {
      section(section) {
        sections.push(section)
        return () => {}
      },
    },
    effect(fn) {
      fn()
    },
    on() {},
  }
  apply(ctx, {
    dbPath: ':memory:',
    projectId: 'unit',
    tokenBudget: 128,
    cliPath: '/dev/null/ai-memory',
  })
  const names = registered.map((t) => t.name)
  for (const name of [
    'memory_remember',
    'memory_recall',
    'memory_forget',
    'memory_pin',
    'memory_consolidate',
  ]) {
    assert.ok(names.includes(name), name)
  }
  assert.equal(sections[0].name, 'ai-memory:pack')
  assert.equal(typeof sections[0].text, 'function')
})
