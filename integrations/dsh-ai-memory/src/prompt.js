/** Best-effort last user utterance from a dsh AssembleContext / inbox message. */
export function textFromUserMessage(message) {
  if (message == null) return ''
  if (typeof message === 'string') return message.trim()
  if (typeof message.content === 'string') return message.content.trim()
  if (Array.isArray(message.content)) {
    return message.content
      .map((part) => {
        if (typeof part === 'string') return part
        if (part && typeof part.text === 'string') return part.text
        return ''
      })
      .join(' ')
      .trim()
  }
  if (typeof message.text === 'string') return message.text.trim()
  if (message.data) return textFromUserMessage(message.data)
  return ''
}

export function lastUserQuery(assembleContext, fallback = '') {
  const agent = assembleContext && assembleContext.agent
  const events = agent && agent.session && agent.session.events
  if (Array.isArray(events)) {
    for (let i = events.length - 1; i >= 0; i -= 1) {
      const ev = events[i]
      if (ev && ev.type === 'user/message') {
        const text = textFromUserMessage(ev.data != null ? ev.data : ev)
        if (text) return text
      }
    }
  }
  const inbox = agent && agent.inbox
  const pending = inbox && (inbox.nextTurn || inbox.pending || inbox.messages)
  if (Array.isArray(pending)) {
    for (let i = pending.length - 1; i >= 0; i -= 1) {
      const text = textFromUserMessage(pending[i])
      if (text) return text
    }
  }
  return fallback || ''
}
