import { useLayoutEffect, useSyncExternalStore } from 'react'
import type { createRunInteractionInbox } from '../runInteractionInbox'

/** Bind pending interactions to the conversation actually rendered by Chat. */
export function useRunInteractionSnapshot(
  inbox: ReturnType<typeof createRunInteractionInbox>,
  conversationId: string | null,
) {
  // A conversation can be opened without the sidebar navigation path (for
  // example, an app starts a chat). Activate before paint, independently of
  // preview restoration, so an already queued or newly arriving prompt shows.
  useLayoutEffect(() => { inbox.activate(conversationId) }, [inbox, conversationId])
  return useSyncExternalStore(inbox.subscribe, inbox.getSnapshot, inbox.getSnapshot)
}
