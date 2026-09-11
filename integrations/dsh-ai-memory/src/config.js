import os from 'node:os'
import path from 'node:path'

export const DEFAULTS = {
  dbPath: '',
  projectId: 'dsh',
  tokenBudget: 8192,
  policy: 'chat',
  prefetchEnabled: true,
  sectionOrder: 40,
  cliPath: '',
}

const POLICIES = new Set(['chat', 'journal', 'default'])

/** Validate / fill plugin config. Throws on invalid budget or policy. */
export function resolveConfig(raw = {}) {
  const tokenBudget = Number(raw.tokenBudget ?? DEFAULTS.tokenBudget)
  if (!Number.isFinite(tokenBudget) || tokenBudget < 1) {
    throw new Error(`tokenBudget must be a positive number, got ${raw.tokenBudget}`)
  }
  const policy = String(raw.policy ?? DEFAULTS.policy)
  if (!POLICIES.has(policy)) {
    throw new Error(`policy must be chat|journal|default, got ${policy}`)
  }
  const projectId = String(raw.projectId ?? DEFAULTS.projectId).trim()
  if (!projectId) {
    throw new Error('projectId must not be empty')
  }
  const prefetchEnabled = raw.prefetchEnabled !== false
  const sectionOrder = Number(raw.sectionOrder ?? DEFAULTS.sectionOrder)
  if (!Number.isFinite(sectionOrder)) {
    throw new Error(`sectionOrder must be a finite number, got ${raw.sectionOrder}`)
  }
  return {
    dbPath: String(raw.dbPath ?? DEFAULTS.dbPath),
    projectId,
    tokenBudget: Math.floor(tokenBudget),
    policy,
    prefetchEnabled,
    sectionOrder,
    cliPath: String(raw.cliPath ?? DEFAULTS.cliPath),
  }
}

export function defaultDbPath() {
  return path.join(os.homedir(), '.local', 'share', 'ai-memory', 'dsh.db')
}

export function resolveDbPath(dbPath) {
  const raw = String(dbPath || process.env.AI_MEMORY_DB || '').trim() || defaultDbPath()
  if (raw === ':memory:') return raw
  if (raw.startsWith('~/')) {
    return path.join(os.homedir(), raw.slice(2))
  }
  return path.resolve(raw)
}

/** Schemastery schema when the host package is present. */
export function createConfigSchema(Schema) {
  return Schema.object({
    dbPath: Schema.string().default(''),
    projectId: Schema.string().default(DEFAULTS.projectId),
    tokenBudget: Schema.number().default(DEFAULTS.tokenBudget),
    policy: Schema.union(['chat', 'journal', 'default']).default('chat'),
    prefetchEnabled: Schema.boolean().default(true),
    sectionOrder: Schema.number().default(DEFAULTS.sectionOrder),
    cliPath: Schema.string().default(''),
  })
}
