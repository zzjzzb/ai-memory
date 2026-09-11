/**
 * Drive the dsh-ai-memory Cordis plugin the same way a harness would:
 * apply(ctx) → ctx.tools.execute + ctx.systemPrompt.section('ai-memory:pack').
 * Memory logic stays in Rust (napi HostSession or ai-memory CLI).
 */
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { createBridge, resolveCliPath } from '../../../integrations/dsh-ai-memory/src/bridge.js'
import { apply } from '../../../integrations/dsh-ai-memory/src/index.js'
import { prefetchArgs } from '../../../integrations/dsh-ai-memory/src/tools.js'

const here = path.dirname(fileURLToPath(import.meta.url))
export const SCENARIO_ROOT = path.resolve(here, '..')
export const DEFAULT_SEED = path.join(SCENARIO_ROOT, 'seed', 'tickets.json')

export function loadSeed(seedPath = DEFAULT_SEED) {
  return JSON.parse(fs.readFileSync(seedPath, 'utf8'))
}

export function charsPer4(text) {
  const n = [...String(text ?? '')].length
  return Math.ceil(n / 4)
}

export function transcriptTexts(seed) {
  const texts = []
  for (const ticket of seed.tickets) {
    for (const turn of ticket.turns) texts.push(turn.text)
    for (const fact of ticket.facts || []) texts.push(fact.text)
  }
  const tmpl = seed.inflate && seed.inflate.text
  const count = (seed.inflate && seed.inflate.count) || 0
  for (let n = 1; n <= count; n += 1) {
    texts.push(String(tmpl).replaceAll('{n}', String(n)))
  }
  return texts
}

export function probeHost() {
  const native = createBridge(
    { dbPath: path.join(os.tmpdir(), 'ai-memory-probe.db'), projectId: 'probe', policy: 'chat' },
  )
  return { kind: native.kind, cliPath: resolveCliPath('') }
}

/** Fake Cordis host: tools + systemPrompt only (no dsh Web UI). */
export function createPluginHost(config) {
  const tools = new Map()
  let section
  apply(
    {
      tools: {
        register(tool) {
          tools.set(tool.name, tool)
        },
      },
      systemPrompt: {
        section(spec) {
          section = spec
          return () => {}
        },
      },
      effect(fn) {
        fn()
      },
      on() {},
    },
    config,
  )
  return {
    tools,
    section,
    async execute(name, args) {
      const tool = tools.get(name)
      if (!tool) throw new Error(`tool not registered: ${name}`)
      return tool.execute(args)
    },
    packText(assembleContext = {}) {
      if (!section || typeof section.text !== 'function') {
        throw new Error('ai-memory:pack section was not registered')
      }
      return section.text(assembleContext)
    },
  }
}

function assembleFor(query) {
  return {
    agent: {
      session: {
        events: [{ type: 'user/message', data: { content: [{ type: 'text', text: query }] } }],
      },
    },
  }
}

function requireOk(result, label) {
  if (!result || result.ok !== true) {
    throw new Error(`${label} failed: ${JSON.stringify(result)}`)
  }
  return result
}

async function rememberAndMaybePin(host, item) {
  const saved = requireOk(
    await host.execute('memory_remember', { text: item.text, tier: item.tier || 'working' }),
    'memory_remember',
  )
  const id = saved.data && saved.data.id
  if (item.pin) {
    if (!id) throw new Error('remember did not return id for pin')
    requireOk(await host.execute('memory_pin', { memory_id: id }), 'memory_pin')
  }
  return { id, pinned: Boolean(item.pin), text: item.text, tier: item.tier || 'working' }
}

/**
 * Seed → remember/pin (plugin tools) → budgeted prefetch (section + host op).
 * `dbPath` defaults to a unique temp file so two projects share one SQLite db.
 */
export async function runScenario(options = {}) {
  const seed = options.seed || loadSeed(options.seedPath)
  const dbPath = options.dbPath || path.join(
    os.tmpdir(),
    `dsh-support-agent-${process.pid}-${Date.now()}.db`,
  )
  const supportId = 'sme-support'
  const hrId = 'sme-hr'
  const tokenBudget = Number(options.tokenBudget ?? seed.tokenBudget)
  const tightBudget = Number(options.tightBudget ?? seed.tightBudget)
  const policy = seed.projects[supportId].policy

  const support = createPluginHost({
    dbPath,
    projectId: supportId,
    policy,
    tokenBudget,
  })
  const hr = createPluginHost({
    dbPath,
    projectId: hrId,
    policy: seed.projects[hrId].policy,
    tokenBudget,
  })
  const supportBridge = createBridge({ dbPath, projectId: supportId, policy })
  const hrBridge = createBridge({ dbPath, projectId: hrId, policy: seed.projects[hrId].policy })

  const remembered = []
  for (const ticket of seed.tickets) {
    if (ticket.project !== supportId) continue
    for (const turn of ticket.turns) {
      remembered.push(await rememberAndMaybePin(support, turn))
    }
    for (const fact of ticket.facts || []) {
      remembered.push(await rememberAndMaybePin(support, fact))
    }
  }
  const inflate = seed.inflate || { count: 0, text: '' }
  for (let n = 1; n <= inflate.count; n += 1) {
    const text = String(inflate.text).replaceAll('{n}', String(n))
    remembered.push(await rememberAndMaybePin(support, { text, tier: 'working' }))
  }
  for (const note of seed.isolationNotes || []) {
    remembered.push({
      ...(await rememberAndMaybePin(hr, note)),
      project: hrId,
    })
  }

  const sidebarQuery = seed.observe.sidebarQuery
  const billingQuery = seed.observe.billingQuery
  const isolationQuery = seed.observe.isolationQuery

  const sidebarSection = support.packText(assembleFor(sidebarQuery))
  const sidebarPack = requireOk(
    supportBridge.dispatch('prefetch_within_budget', prefetchArgs(sidebarQuery, tokenBudget)),
    'prefetch sidebar',
  )
  const tightPack = requireOk(
    supportBridge.dispatch('prefetch_within_budget', prefetchArgs(billingQuery, tightBudget)),
    'prefetch tight billing',
  )
  const isolationPack = requireOk(
    hrBridge.dispatch('prefetch_within_budget', prefetchArgs(isolationQuery, tokenBudget)),
    'prefetch isolation',
  )
  const isolationSection = hr.packText(assembleFor(isolationQuery))
  const recall = requireOk(
    await support.execute('memory_recall', { text: sidebarQuery, limit: 4 }),
    'memory_recall',
  )

  const transcript = transcriptTexts(seed).join('\n')
  const report = {
    dbPath,
    bridge: supportBridge.kind,
    tools: [...support.tools.keys()],
    sectionName: support.section && support.section.name,
    remembered: remembered.length,
    transcriptChars: [...transcript].length,
    transcriptTokens: charsPer4(transcript),
    tokenBudget,
    tightBudget,
    pinMarker: seed.pinMarker,
    sidebar: {
      query: sidebarQuery,
      section: sidebarSection,
      tokens: sidebarPack.data.tokens,
      maxTokens: sidebarPack.data.max_tokens,
      text: sidebarPack.data.text,
      hits: (sidebarPack.data.blocks || []).length,
    },
    tight: {
      query: billingQuery,
      tokens: tightPack.data.tokens,
      maxTokens: tightPack.data.max_tokens,
      text: tightPack.data.text,
    },
    isolation: {
      query: isolationQuery,
      tokens: isolationPack.data.tokens,
      text: isolationPack.data.text,
      section: isolationSection,
    },
    recallContext: recall.data && recall.data.context,
  }
  return report
}

export function assertScenarioInvariants(report, seed = loadSeed()) {
  const failures = []
  if (report.sidebar.tokens > report.tokenBudget) {
    failures.push(`sidebar pack ${report.sidebar.tokens} > budget ${report.tokenBudget}`)
  }
  if (report.tight.tokens > report.tightBudget) {
    failures.push(`tight pack ${report.tight.tokens} > budget ${report.tightBudget}`)
  }
  if (report.transcriptTokens <= report.tokenBudget) {
    failures.push(
      `transcript tokens ${report.transcriptTokens} should exceed budget ${report.tokenBudget}`,
    )
  }
  if (!String(report.tight.text).includes(seed.pinMarker)) {
    failures.push(`pin ${seed.pinMarker} missing from tight pack`)
  }
  if (!/sidebar/i.test(report.sidebar.text) && !/T-1042/.test(report.sidebar.text)) {
    failures.push('sidebar pack missing T-1042 / sidebar content')
  }
  if (/T-1042|sidebar overlap/i.test(report.isolation.text)) {
    failures.push('HR project leaked support sidebar ticket')
  }
  if (!/handbook|HR only/i.test(report.isolation.text)) {
    failures.push('HR pack missing isolation notes')
  }
  if (report.sectionName !== 'ai-memory:pack') {
    failures.push(`expected section ai-memory:pack, got ${report.sectionName}`)
  }
  const required = [
    'memory_remember',
    'memory_recall',
    'memory_forget',
    'memory_pin',
    'memory_consolidate',
    'memory_compact',
  ]
  for (const name of required) {
    if (!report.tools.includes(name)) failures.push(`missing tool ${name}`)
  }
  if (failures.length) {
    const err = new Error(failures.join('; '))
    err.failures = failures
    throw err
  }
}
