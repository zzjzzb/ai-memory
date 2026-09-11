/** Tool names aligned with `ai_memory::memory_tool_specs`. */
export const TOOL_REMEMBER = 'memory_remember'
export const TOOL_RECALL = 'memory_recall'
export const TOOL_FORGET = 'memory_forget'
export const TOOL_PIN = 'memory_pin'
export const TOOL_CONSOLIDATE = 'memory_consolidate'
/** Extra explicit fold — same as `AgentSession::compact_working`. */
export const TOOL_COMPACT = 'memory_compact'

export const HARNESS_TOOL_NAMES = [
  TOOL_REMEMBER,
  TOOL_RECALL,
  TOOL_FORGET,
  TOOL_PIN,
  TOOL_CONSOLIDATE,
]

/**
 * Map dsh tool args onto the Rust host JSON. Unknown keys are dropped so the
 * crate's `dispatch_tool` sees the same shape as `call_tool`.
 */
export function mapToolArgs(name, args = {}) {
  switch (name) {
    case TOOL_REMEMBER:
      return pick(args, ['text', 'tier', 'metadata'])
    case TOOL_RECALL:
      return pick(args, ['text', 'limit'])
    case TOOL_FORGET:
    case TOOL_PIN:
      return pick(args, ['memory_id'])
    case TOOL_CONSOLIDATE:
    case TOOL_COMPACT:
      return {}
    default:
      throw new Error(`unknown memory tool: ${name}`)
  }
}

export function prefetchArgs(query, tokenBudget) {
  return {
    query: query == null ? '' : String(query),
    max_tokens: Number(tokenBudget),
  }
}

export function toolOutputText(result) {
  if (!result || typeof result !== 'object') return String(result ?? '')
  if (result.ok === false) return result.error || 'memory tool failed'
  const data = result.data
  if (data && typeof data.context === 'string') return data.context
  if (data && typeof data.text === 'string' && result.name === 'prefetch_within_budget') {
    return data.text
  }
  try {
    return JSON.stringify(data ?? result)
  } catch {
    return String(data)
  }
}

export const TOOL_SPECS = [
  {
    name: TOOL_REMEMBER,
    description:
      'Store a memory in the current ai-memory project. Optional tier: working, episodic, profile. Does not call an LLM.',
    parameters: {
      text: { type: 'string', required: true, description: 'Memory text to store' },
      tier: { type: 'string', enum: ['working', 'episodic', 'profile'] },
    },
  },
  {
    name: TOOL_RECALL,
    description:
      'Recall memories in the current project (hybrid time + keyword + vector). Project-scoped; no cross-project leak.',
    parameters: {
      text: { type: 'string', required: true, description: 'Query text' },
      limit: { type: 'number', description: 'Max hits (default 8)' },
    },
  },
  {
    name: TOOL_FORGET,
    description: 'Delete a memory by id in the current project.',
    parameters: {
      memory_id: { type: 'string', required: true },
    },
  },
  {
    name: TOOL_PIN,
    description: 'Pin a memory so retention TTL will not expire it and budgeted packs prefer it.',
    parameters: {
      memory_id: { type: 'string', required: true },
    },
  },
  {
    name: TOOL_CONSOLIDATE,
    description:
      'Apply this project MemoryPolicy (TTL delete + working→episodic→profile). Explicit — never automatic.',
    parameters: {},
  },
  {
    name: TOOL_COMPACT,
    description:
      'Fold older unpinned working notes into one extractive episodic note (offline, not an LLM summary). Explicit.',
    parameters: {},
  },
]

function pick(obj, keys) {
  const out = {}
  for (const key of keys) {
    if (obj[key] !== undefined) out[key] = obj[key]
  }
  return out
}
