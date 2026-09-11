import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  assertScenarioInvariants,
  charsPer4,
  loadSeed,
  runScenario,
  transcriptTexts,
} from '../lib/driver.mjs'

const seed = loadSeed()

test('seed → remember → budgeted prefetch (Cordis plugin host, no dsh UI)', async () => {
  const report = await runScenario({ seed })
  assertScenarioInvariants(report, seed)

  assert.ok(report.remembered > seed.inflate.count, 'seeded turns + inflate + isolation')
  assert.ok(report.sidebar.tokens <= report.tokenBudget)
  assert.ok(report.tight.tokens <= report.tightBudget)
  assert.match(report.tight.text, new RegExp(seed.pinMarker))
  assert.match(report.sidebar.section, /## Memory \(project: sme-support/)
  assert.doesNotMatch(report.isolation.text, /T-1042/)
  assert.doesNotMatch(report.isolation.section, /sidebar overlap/i)
  assert.ok(report.transcriptTokens > report.tokenBudget)

  const dumped = transcriptTexts(seed).join('\n')
  assert.equal(report.transcriptTokens, charsPer4(dumped))
  assert.ok(charsPer4(report.sidebar.text) <= report.tokenBudget + 1)
})
