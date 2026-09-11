#!/usr/bin/env node
/**
 * Headless SME support-agent scenario (no dsh Web UI).
 *
 * Drives the Cordis plugin the same way dsh would: apply(ctx) registers
 * memory_* tools and injects a budgeted `ai-memory:pack` systemPrompt section.
 * Rust remains the store (napi or `ai-memory` CLI).
 *
 *   node scenarios/dsh-support-agent/sim/run.mjs
 *   npm run sim --prefix scenarios/dsh-support-agent
 */
import { assertScenarioInvariants, loadSeed, probeHost, runScenario } from '../lib/driver.mjs'

function line(title) {
  console.log(`\n== ${title} ==`)
}

async function main() {
  const seed = loadSeed()
  const host = probeHost()
  console.log(`dsh-support-agent  (${seed.title})`)
  console.log(`bridge: ${host.kind}   cli: ${host.cliPath}`)
  console.log('This is the Cordis plugin path, not a JS memory rewrite.')

  const report = await runScenario({ seed })
  assertScenarioInvariants(report, seed)

  line('story')
  console.log(seed.summary)
  console.log(`tickets: ${seed.tickets.map((t) => t.id).join(', ')}`)
  console.log(`remembered rows: ${report.remembered}`)
  console.log(`db: ${report.dbPath}`)

  line('do not dump the transcript')
  console.log(
    `raw session ≈ ${report.transcriptTokens} tokens (${report.transcriptChars} chars, ceil/4)`,
  )
  console.log(`plugin tokenBudget: ${report.tokenBudget}   tight pin budget: ${report.tightBudget}`)
  console.log('A ~1M (or smaller) model window still cannot take the whole chat; pack a slice.')

  line(`tools (${report.tools.length}) + ${report.sectionName}`)
  console.log(report.tools.join(', '))

  line(`budgeted pack  query=${JSON.stringify(report.sidebar.query)}`)
  console.log(
    `tokens=${report.sidebar.tokens}  max=${report.sidebar.maxTokens}  hits=${report.sidebar.hits}`,
  )
  console.log(report.sidebar.section.trimEnd())

  line(`pins survive tight budget  query=${JSON.stringify(report.tight.query)}`)
  console.log(`tokens=${report.tight.tokens}  max=${report.tight.maxTokens}`)
  console.log(report.tight.text.trimEnd())
  console.log(`pin marker present: ${report.tight.text.includes(report.pinMarker)}`)

  line('project isolation (sme-hr must not see T-1042)')
  console.log(report.isolation.section.trimEnd())

  line('observe')
  console.log('✓ pack tokens stay under budget (prefetch_within_budget)')
  console.log('✓ pinned billing owner survives the tight pack')
  console.log('✓ sme-hr recall does not leak the support sidebar ticket')
  console.log('✓ tools + ai-memory:pack match the dsh-ai-memory Cordis plugin')
  console.log('\nReal dsh: docs/INSTALL_DSH.md (`dsh plugin add github:zzjzzb/ai-memory`). dsh.pub submit is optional.')
}

main().catch((err) => {
  console.error(err)
  process.exitCode = 1
})
