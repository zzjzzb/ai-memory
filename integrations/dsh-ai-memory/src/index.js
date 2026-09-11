import { createRequire } from 'node:module'
import { createBridge } from './bridge.js'
import { createConfigSchema, resolveConfig, resolveDbPath } from './config.js'
import { lastUserQuery, textFromUserMessage } from './prompt.js'
import { mapToolArgs, prefetchArgs, TOOL_SPECS, toolOutputText } from './tools.js'

const require = createRequire(import.meta.url)

export const name = 'dsh-ai-memory'
export const inject = ['tools', 'systemPrompt']

function optionalRequire(id) {
  try {
    return require(id)
  } catch {
    return null
  }
}

const SchemaMod = optionalRequire('@deepseek-ai/schemastery')
const Schema = SchemaMod && (SchemaMod.default || SchemaMod)
export const Config = Schema ? createConfigSchema(Schema) : undefined

const toolsMod = optionalRequire('@deepseek-ai/dsh-tools')
const defineTool = toolsMod && (toolsMod.defineTool || (toolsMod.default && toolsMod.default.defineTool))

export function apply(ctx, rawConfig = {}) {
  const config = resolveConfig(rawConfig)
  const resolved = { ...config, dbPath: resolveDbPath(config.dbPath) }
  const bridge = createBridge(resolved)
  let lastQuery = ''

  ctx.effect(() => () => {
    if (typeof bridge.close === 'function') bridge.close()
  })

  if (!ctx.tools || typeof ctx.tools.register !== 'function') {
    throw new Error('dsh-ai-memory requires ctx.tools (declare inject = ["tools"])')
  }

  for (const spec of TOOL_SPECS) {
    ctx.tools.register(asTool(spec, bridge, (query) => {
      lastQuery = query || lastQuery
    }))
  }

  listenForUserTurns(ctx, (text) => {
    lastQuery = text
  })

  if (resolved.prefetchEnabled && ctx.systemPrompt && typeof ctx.systemPrompt.section === 'function') {
    ctx.systemPrompt.section({
      name: 'ai-memory:pack',
      order: resolved.sectionOrder,
      text: (assembleContext = {}) => {
        const query = lastUserQuery(assembleContext, lastQuery)
        const result = bridge.dispatch('prefetch_within_budget', prefetchArgs(query, resolved.tokenBudget))
        if (result && result.ok && result.data && typeof result.data.text === 'string') {
          return result.data.text
        }
        const err = result && result.error ? ` (${result.error})` : ''
        return `## Memory (project: ${resolved.projectId}, 0 hits)\n(none)${err}`
      },
    })
  }
}

function asTool(spec, bridge, onRecallQuery) {
  const body = {
    name: spec.name,
    description: spec.description,
    parameters: spec.parameters,
    output: {
      schema: { type: 'object' },
      render: (_args, value) => [{ type: 'text', text: toolOutputText(value) }],
    },
    async execute(args) {
      const mapped = mapToolArgs(spec.name, args || {})
      if (spec.name === 'memory_recall' && mapped.text) onRecallQuery(mapped.text)
      if (spec.name === 'memory_remember' && mapped.text) onRecallQuery(mapped.text)
      return bridge.dispatch(spec.name, mapped)
    },
  }
  return defineTool ? defineTool(body) : body
}

function listenForUserTurns(ctx, onText) {
  if (typeof ctx.on !== 'function') return
  try {
    ctx.on('agent/pre-step', async (event, next) => {
      const messages = event && event.messages
      if (Array.isArray(messages)) {
        for (let i = messages.length - 1; i >= 0; i -= 1) {
          const text = textFromUserMessage(messages[i])
          if (text) {
            onText(text)
            break
          }
        }
      }
      return next()
    })
  } catch {
    // Host without this event — section provider + tools still work.
  }
}

export {
  createBridge,
  createConfigSchema,
  lastUserQuery,
  mapToolArgs,
  prefetchArgs,
  resolveConfig,
  resolveDbPath,
  TOOL_SPECS,
}
