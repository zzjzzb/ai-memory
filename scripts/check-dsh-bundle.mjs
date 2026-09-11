#!/usr/bin/env node
/**
 * CI-friendly assertion: the repository root is a valid dsh bundle so
 * `dsh plugin add github:zzjzzb/ai-memory` can activate a layer.
 *
 * Usage (from repo root): node scripts/check-dsh-bundle.mjs
 */
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath, pathToFileURL } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const require = createRequire(import.meta.url)
const pkg = require('../package.json')
const nestedPkg = require('../integrations/dsh-ai-memory/package.json')

function read(rel) {
  return fs.readFileSync(path.join(root, rel), 'utf8')
}

assert.equal(pkg.name, 'dsh-ai-memory', 'root package name must match patch row name')
assert.equal(pkg.type, 'module')
assert.ok(pkg.dsh && pkg.dsh.bundle && pkg.dsh.bundle.patch, 'missing dsh.bundle.patch')
assert.equal(pkg.dsh.bundle.patch, './cordis.patch.yml')
assert.equal(
  pkg.scripts.prepare,
  'node integrations/dsh-ai-memory/scripts/build-native.mjs',
  'prepare must build napi + CLI from this checkout (git install does not run `build`)',
)

const patchRel = pkg.dsh.bundle.patch
const patchPath = path.join(root, patchRel)
assert.ok(fs.existsSync(patchPath), `patch file missing: ${patchRel}`)
const patch = read(patchRel)
assert.match(patch, /id:\s*dsh-ai-memory/)
assert.match(patch, /name:\s*dsh-ai-memory/)

const nestedPatch = read('integrations/dsh-ai-memory/cordis.patch.yml')
assert.equal(
  patch.trim(),
  nestedPatch.trim(),
  'root and integrations/dsh-ai-memory cordis.patch.yml must stay identical',
)

assert.equal(nestedPkg.name, 'dsh-ai-memory')
assert.equal(nestedPkg.dsh.bundle.patch, './cordis.patch.yml')
assert.ok(
  Array.isArray(nestedPkg.files) && nestedPkg.files.includes('ai-memory.node'),
  'nested package files must include prepare output ai-memory.node',
)
assert.ok(
  Array.isArray(pkg.files) && pkg.files.includes('integrations/dsh-ai-memory'),
  'root files must include the Cordis host package so git pack keeps JS + native output',
)

const indexPath = path.join(root, 'index.js')
assert.ok(fs.existsSync(indexPath), 'root index.js (Host re-export) missing')

const mod = await import(pathToFileURL(indexPath).href)
assert.equal(mod.name, 'dsh-ai-memory')
assert.equal(typeof mod.apply, 'function')
assert.ok(Array.isArray(mod.inject) && mod.inject.includes('tools'))

console.log('check-dsh-bundle: ok (root package.json is an installable dsh bundle)')
