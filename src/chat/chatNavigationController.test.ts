// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createChatNavigationController } from './chatNavigationController'
import { getConversationTransitionSnapshot, invalidateConversationTransition } from './conversationTransitionStore'
import type { Conversation } from './types'

vi.mock('./persistence', () => ({ forgetRememberedChatRoute: vi.fn() }))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

function conversation(id: string): Conversation {
  return {
    id, revision: 1, title: id, provider_id: 'test', model: 'test',
    messages: [], created_at: 1, updated_at: 1,
  }
}

function setup() {
  let current: Conversation | null = null
  let inFlight = false
  const reads = new Map<string, ReturnType<typeof deferred<Conversation>>>()
  const readStarted = deferred<string>()
  const ownership = deferred<ReadonlySet<string>>()
  const listPopouts = vi.fn<(refresh?: boolean) => Promise<ReadonlySet<string>>>(() => ownership.promise)
  const shown: string[] = []
  const errors: string[] = []
  const occupyPopout = vi.fn()
  const prepareNewConversation = vi.fn()
  const clearEmptyChat = vi.fn()
  const requestClearChat = vi.fn((): 'busy' | 'cancelled' | 'confirmed' => 'confirmed')
  const deleteConversation = vi.fn(() => Promise.resolve())
  const cancelDeletedRun = vi.fn(() => Promise.resolve())
  const finalizeDeletedChat = vi.fn((conversationId: string, clearCurrentView: boolean) => {
    if (clearCurrentView && current?.id === conversationId) current = null
  })
  const reportClearError = vi.fn()
  const controller = createChatNavigationController({
    currentConversation: () => current,
    currentConversationId: () => current?.id ?? null,
    listPopouts,
    readConversation: (id) => {
      const pending = deferred<Conversation>()
      reads.set(id, pending)
      readStarted.resolve(id)
      return pending.promise
    },
    isConversationInFlight: () => inFlight,
    prepareNewConversation,
    clearEmptyChat,
    requestClearChat,
    deleteConversation,
    cancelDeletedRun,
    finalizeDeletedChat,
    reportClearError,
    focusPopout: vi.fn(),
    occupyPopout,
    prepareSelection: vi.fn(),
    showConversation: (value) => {
      current = value
      shown.push(value.id)
    },
    resetConversation: () => { current = null },
    reportLoadError: (_id, error) => { errors.push(error.message) },
  })
  return {
    controller, ownership, listPopouts, reads, readStarted, shown, errors, occupyPopout,
    prepareNewConversation, clearEmptyChat, requestClearChat, deleteConversation,
    cancelDeletedRun, finalizeDeletedChat, reportClearError,
    setCurrent: (value: Conversation | null) => { current = value },
    setInFlight: (value: boolean) => { inFlight = value },
  }
}

describe('chat navigation controller', () => {
  beforeEach(() => {
    invalidateConversationTransition()
    window.location.hash = '#chat'
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it.each(['select', 'route', 'reload', 'already-open'] as const)(
    'settles %s when the popout lookup rejects', async (entry) => {
      const state = setup()
      if (entry === 'reload' || entry === 'already-open') state.setCurrent(conversation('a'))
      const pending = entry === 'route' ? state.controller.loadRouteConversation('a')
        : entry === 'reload' ? state.controller.reloadConversation('a', { force: true })
          : state.controller.selectConversation('a')
      state.ownership.reject(new Error('window lookup failed'))
      await expect(pending).resolves.toBeUndefined()
      expect(getConversationTransitionSnapshot().loading).toBe(false)
      expect(state.errors).toEqual(['window lookup failed'])
      expect(state.reads.size).toBe(0)
    },
  )

  it('stops waiting for a hung ownership lookup and ignores its late result', async () => {
    vi.useFakeTimers()
    const state = setup()
    const pending = state.controller.selectConversation('a')
    await vi.advanceTimersByTimeAsync(10_000)
    expect(getConversationTransitionSnapshot().loading).toBe(false)
    await pending
    expect(state.errors[0]).toContain('超时')
    state.ownership.resolve(new Set())
    await Promise.resolve()
    expect(state.reads.size).toBe(0)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('starts a fresh ownership lookup when retrying a hung lookup', async () => {
    vi.useFakeTimers()
    const state = setup()
    const first = state.controller.selectConversation('a')
    await vi.advanceTimersByTimeAsync(10_000)
    await first
    state.listPopouts.mockResolvedValueOnce(new Set())
    const retry = state.controller.selectConversation('a')
    await state.readStarted.promise
    expect(state.listPopouts).toHaveBeenLastCalledWith(true)
    state.reads.get('a')!.resolve(conversation('a'))
    await retry
    state.ownership.resolve(new Set(['a']))
    await vi.advanceTimersByTimeAsync(0)
    expect(state.shown).toEqual(['a'])
    expect(state.occupyPopout).not.toHaveBeenCalled()
    expect(getConversationTransitionSnapshot().loading).toBe(false)
  })

  it('bounds ownership and reading together, preserves the previous view, and allows retry', async () => {
    vi.useFakeTimers()
    const state = setup()
    state.setCurrent(conversation('previous'))
    window.location.hash = '#chat/previous'
    const pending = state.controller.selectConversation('a')
    await vi.advanceTimersByTimeAsync(6_000)
    state.ownership.resolve(new Set())
    await state.readStarted.promise
    const oldRead = state.reads.get('a')!
    await vi.advanceTimersByTimeAsync(4_000)
    expect(getConversationTransitionSnapshot().loading).toBe(false)
    await pending
    expect(state.errors[0]).toContain('超时')
    expect(state.shown).toEqual([])
    expect(window.location.hash).toBe('#chat/previous')
    expect(state.finalizeDeletedChat).not.toHaveBeenCalled()
    expect(state.cancelDeletedRun).not.toHaveBeenCalled()

    const retry = state.controller.selectConversation('a')
    await vi.advanceTimersByTimeAsync(0)
    state.reads.get('a')!.resolve(conversation('a'))
    await retry
    oldRead.resolve(conversation('old-a'))
    await vi.advanceTimersByTimeAsync(0)
    expect(state.shown).toEqual(['a'])
    expect(window.location.hash).toBe('#chat/a')
    expect(getConversationTransitionSnapshot().loading).toBe(false)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('does not let an old timeout cancel a newer conversation load', async () => {
    vi.useFakeTimers()
    const state = setup()
    state.ownership.resolve(new Set())
    const first = state.controller.selectConversation('a')
    await state.readStarted.promise
    await vi.advanceTimersByTimeAsync(5_000)
    const second = state.controller.selectConversation('b')
    await vi.advanceTimersByTimeAsync(5_000)
    await first
    expect(state.errors).toEqual([])
    expect(getConversationTransitionSnapshot()).toMatchObject({ loading: true, targetConversationId: 'b' })
    state.reads.get('b')!.resolve(conversation('b'))
    await second
    state.reads.get('a')!.reject(new Error('late read failure'))
    await vi.advanceTimersByTimeAsync(0)
    expect(state.shown).toEqual(['b'])
    expect(state.errors).toEqual([])
    expect(vi.getTimerCount()).toBe(0)
  })

  it('retries a failed refresh even when the conversation is already displayed', async () => {
    const state = setup()
    state.setCurrent(conversation('a'))
    state.ownership.resolve(new Set())
    const failed = state.controller.reloadConversation('a', { force: true })
    await state.readStarted.promise
    state.reads.get('a')!.reject(new Error('temporary read failure'))
    await failed
    expect(state.errors).toEqual(['temporary read failure'])
    const oldRead = state.reads.get('a')
    const retry = state.controller.selectConversation('a')
    await vi.waitFor(() => expect(state.reads.get('a')).not.toBe(oldRead))
    state.reads.get('a')!.resolve(conversation('a'))
    await retry
    expect(state.shown).toEqual(['a'])
    expect(getConversationTransitionSnapshot().loading).toBe(false)
  })

  it('settles an empty conversation without waiting for animation frames', async () => {
    const frame = vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 1)
    try {
      const state = setup()
      const loading = state.controller.loadRouteConversation('empty')
      state.ownership.resolve(new Set())
      await state.readStarted.promise
      state.reads.get('empty')!.resolve(conversation('empty'))
      await loading
      expect(getConversationTransitionSnapshot().loading).toBe(false)
      expect(frame).not.toHaveBeenCalled()
    } finally {
      frame.mockRestore()
    }
  })

  it('settles selection of a conversation owned by a popout without reading it', async () => {
    const state = setup()
    const selecting = state.controller.selectConversation('a')
    state.ownership.resolve(new Set(['a']))
    await selecting
    expect(state.occupyPopout).toHaveBeenCalledWith('a')
    expect(state.reads.size).toBe(0)
    expect(getConversationTransitionSnapshot().loading).toBe(false)
  })

  it('does not reopen a created conversation after New invalidates its pending creation', async () => {
    const { controller, shown } = setup()
    const created = deferred<Conversation>()
    const permit = controller.beginConversationCreation()
    const completion = created.promise.then((value) => controller.commitCreatedConversation(permit, value))

    controller.startNewConversation()
    created.resolve(conversation('late'))

    expect(await completion).toBe(false)
    expect(shown).toEqual([])
    expect(window.location.hash).toBe('#chat')
  })

  it('keeps Settings visible when a pending creation finishes after leaving Chat', async () => {
    const { controller, shown } = setup()
    const created = deferred<Conversation>()
    const permit = controller.beginConversationCreation()
    const completion = created.promise.then((value) => controller.commitCreatedConversation(permit, value))

    window.location.hash = '#chat/settings'
    created.resolve(conversation('late'))

    expect(await completion).toBe(false)
    expect(shown).toEqual([])
    expect(window.location.hash).toBe('#chat/settings')
  })

  it('only commits the latest creation started from the same empty route', async () => {
    const { controller, shown } = setup()
    const first = deferred<Conversation>()
    const second = deferred<Conversation>()
    const firstPermit = controller.beginConversationCreation()
    const firstCompletion = first.promise.then((value) => controller.commitCreatedConversation(firstPermit, value))
    const secondPermit = controller.beginConversationCreation()
    const secondCompletion = second.promise.then((value) => controller.commitCreatedConversation(secondPermit, value))

    second.resolve(conversation('newer'))
    expect(await secondCompletion).toBe(true)
    first.resolve(conversation('older'))
    expect(await firstCompletion).toBe(false)
    expect(shown).toEqual(['newer'])
    expect(window.location.hash).toBe('#chat/newer')
  })

  it('does not apply a terminal reload after its run commit right expires', async () => {
    const state = setup()
    state.setCurrent(conversation('a'))
    let canCommit = true
    const reloading = state.controller.reloadConversation('a', { force: true, canCommit: () => canCommit })
    state.ownership.resolve(new Set())
    expect(await state.readStarted.promise).toBe('a')
    canCommit = false
    state.reads.get('a')!.resolve(conversation('a'))
    await reloading

    expect(state.shown).toEqual([])
    expect(window.location.hash).toBe('#chat')
  })

  it('does not discard the conversation when a stale terminal reload fails', async () => {
    const state = setup()
    state.setCurrent(conversation('a'))
    let canCommit = true
    const reloading = state.controller.reloadConversation('a', { force: true, canCommit: () => canCommit })
    state.ownership.resolve(new Set())
    expect(await state.readStarted.promise).toBe('a')
    canCommit = false
    state.reads.get('a')!.reject(new Error('old read failed'))
    await reloading

    expect(state.errors).toEqual([])
    expect(state.shown).toEqual([])
    expect(window.location.hash).toBe('#chat')
  })

  it('ignores a late popout ownership result after navigating elsewhere', async () => {
    const { controller, ownership, reads, shown } = setup()
    const selecting = controller.selectConversation('a')
    controller.leaveConversation()
    window.location.hash = '#chat/settings'
    ownership.resolve(new Set())
    await selecting

    expect(reads.size).toBe(0)
    expect(shown).toEqual([])
    expect(window.location.hash).toBe('#chat/settings')
  })

  it('does not let A replace B when their popout ownership lookup settles together', async () => {
    const { controller, ownership, reads, readStarted, shown } = setup()
    const selectingA = controller.selectConversation('a')
    const selectingB = controller.selectConversation('b')
    ownership.resolve(new Set())
    expect(await readStarted.promise).toBe('b')
    reads.get('b')!.resolve(conversation('b'))
    await Promise.all([selectingA, selectingB])

    expect(reads.has('a')).toBe(false)
    expect(shown).toEqual(['b'])
    expect(window.location.hash).toBe('#chat/b')
  })

  it('does not let an old load failure erase a later route', async () => {
    const { controller, ownership, reads, readStarted, errors } = setup()
    const loading = controller.loadRouteConversation('missing-a')
    ownership.resolve(new Set())
    expect(await readStarted.promise).toBe('missing-a')
    controller.leaveConversation()
    window.location.hash = '#chat/settings'
    reads.get('missing-a')!.reject(new Error('missing'))
    await loading

    expect(errors).toEqual([])
    expect(window.location.hash).toBe('#chat/settings')
  })

  it('keeps an already open conversation when clicked again while another load is pending', async () => {
    const { controller, ownership, reads, readStarted, shown, setCurrent } = setup()
    setCurrent(conversation('a'))
    const other = controller.selectConversation('b')
    ownership.resolve(new Set())
    expect(await readStarted.promise).toBe('b')
    await controller.selectConversation('a')
    reads.get('b')!.resolve(conversation('b'))
    await other

    expect(shown).toEqual([])
    expect(reads.has('a')).toBe(false)
    expect(window.location.hash).toBe('#chat/a')
  })

  it('reconciles popout entry by replacing the main-window conversation without reading its messages', async () => {
    const { controller, reads, occupyPopout, setCurrent } = setup()
    setCurrent(conversation('a'))
    await controller.reconcilePopouts(new Set(), new Set(['a']))

    expect(occupyPopout).toHaveBeenCalledWith('a')
    expect(reads.size).toBe(0)
  })

  it('opens a different conversation by route and does not start a duplicate read', async () => {
    const { controller, reads } = setup()
    window.location.hash = '#chat/a'
    await controller.openConversation('b')

    expect(window.location.hash).toBe('#chat/b')
    expect(reads.size).toBe(0)
  })

  it('starts a new draft and invalidates a pending selection before changing route', async () => {
    const { controller, ownership, reads, prepareNewConversation } = setup()
    const selecting = controller.selectConversation('a')

    controller.startNewConversation()
    ownership.resolve(new Set())
    await selecting

    expect(prepareNewConversation).toHaveBeenCalledOnce()
    expect(reads.size).toBe(0)
    expect(window.location.hash).toBe('#chat')
    expect(getConversationTransitionSnapshot().loading).toBe(false)
  })

  it('clears only view feedback when there is no current conversation to delete', async () => {
    const { controller, clearEmptyChat, requestClearChat, deleteConversation } = setup()

    await controller.clearCurrentChat()

    expect(clearEmptyChat).toHaveBeenCalledOnce()
    expect(requestClearChat).not.toHaveBeenCalled()
    expect(deleteConversation).not.toHaveBeenCalled()
  })

  it('does not invalidate a pending selection when clear is blocked by a busy conversation', async () => {
    const { controller, ownership, reads, readStarted, shown, setCurrent, requestClearChat, reportClearError } = setup()
    setCurrent(conversation('a'))
    requestClearChat.mockReturnValue('busy')
    const selecting = controller.selectConversation('b')
    const requestId = getConversationTransitionSnapshot().requestId

    await controller.clearCurrentChat()
    expect(getConversationTransitionSnapshot().requestId).toBe(requestId)
    expect(reportClearError).toHaveBeenCalledWith('a', '请先停止当前回复，再清空对话。')
    ownership.resolve(new Set())
    expect(await readStarted.promise).toBe('b')
    reads.get('b')!.resolve(conversation('b'))
    await selecting
    expect(shown).toEqual(['b'])
  })

  it('does not invalidate a pending selection when clear confirmation is declined', async () => {
    const { controller, ownership, reads, readStarted, shown, setCurrent, requestClearChat, deleteConversation } = setup()
    setCurrent(conversation('a'))
    requestClearChat.mockReturnValue('cancelled')
    const selecting = controller.selectConversation('b')
    const requestId = getConversationTransitionSnapshot().requestId

    await controller.clearCurrentChat()
    expect(getConversationTransitionSnapshot().requestId).toBe(requestId)
    expect(deleteConversation).not.toHaveBeenCalled()
    ownership.resolve(new Set())
    expect(await readStarted.promise).toBe('b')
    reads.get('b')!.resolve(conversation('b'))
    await selecting
    expect(shown).toEqual(['b'])
  })

  it('locally finalizes a late deletion of A without clearing subsequently selected B', async () => {
    const {
      controller, ownership, reads, readStarted, setCurrent, deleteConversation,
      finalizeDeletedChat,
    } = setup()
    setCurrent(conversation('a'))
    window.location.hash = '#chat/a'
    const deleting = deferred<void>()
    deleteConversation.mockReturnValue(deleting.promise)
    const clearing = controller.clearCurrentChat()
    const selecting = controller.selectConversation('b')
    ownership.resolve(new Set())
    expect(await readStarted.promise).toBe('b')
    reads.get('b')!.resolve(conversation('b'))
    await selecting

    deleting.resolve()
    await clearing
    expect(finalizeDeletedChat).toHaveBeenCalledWith('a', false)
    expect(window.location.hash).toBe('#chat/b')
  })

  it('does not cancel B loading when A deletion completes before B has rendered', async () => {
    const {
      controller, ownership, reads, readStarted, setCurrent, deleteConversation,
      finalizeDeletedChat,
    } = setup()
    setCurrent(conversation('a'))
    window.location.hash = '#chat/a'
    const deleting = deferred<void>()
    deleteConversation.mockReturnValue(deleting.promise)
    const clearing = controller.clearCurrentChat()
    const selecting = controller.selectConversation('b')
    ownership.resolve(new Set())
    expect(await readStarted.promise).toBe('b')

    deleting.resolve()
    await clearing
    expect(finalizeDeletedChat).toHaveBeenCalledWith('a', false)
    expect(getConversationTransitionSnapshot().targetConversationId).toBe('b')

    reads.get('b')!.resolve(conversation('b'))
    await selecting
    expect(window.location.hash).toBe('#chat/b')
  })

  it('does not clear A while B was already loading at delete confirmation', async () => {
    const {
      controller, ownership, reads, readStarted, setCurrent, deleteConversation,
      finalizeDeletedChat,
    } = setup()
    setCurrent(conversation('a'))
    window.location.hash = '#chat/a'
    const selecting = controller.selectConversation('b')
    ownership.resolve(new Set())
    expect(await readStarted.promise).toBe('b')
    const deleting = deferred<void>()
    deleteConversation.mockReturnValue(deleting.promise)
    const clearing = controller.clearCurrentChat()

    deleting.resolve()
    await clearing
    expect(finalizeDeletedChat).toHaveBeenCalledWith('a', false)
    expect(getConversationTransitionSnapshot().targetConversationId).toBe('b')
    reads.get('b')!.resolve(conversation('b'))
    await selecting
    expect(window.location.hash).toBe('#chat/b')
  })

  it('preserves the route and local conversation when deletion fails', async () => {
    const { controller, setCurrent, deleteConversation, finalizeDeletedChat, reportClearError } = setup()
    setCurrent(conversation('a'))
    window.location.hash = '#chat/a'
    deleteConversation.mockRejectedValue(new Error('disk denied'))
    const lease = getConversationTransitionSnapshot().requestId

    await controller.clearCurrentChat()

    expect(finalizeDeletedChat).not.toHaveBeenCalled()
    expect(reportClearError).toHaveBeenCalledWith('a', 'disk denied')
    expect(getConversationTransitionSnapshot().requestId).toBe(lease)
    expect(window.location.hash).toBe('#chat/a')
  })

  it('keeps a successful deletion finalized when post-delete cancellation fails', async () => {
    const {
      controller, setCurrent, setInFlight, deleteConversation, cancelDeletedRun,
      finalizeDeletedChat,
      reportClearError,
    } = setup()
    setCurrent(conversation('a'))
    window.location.hash = '#chat/a'
    deleteConversation.mockImplementation(async () => { setInFlight(true) })
    cancelDeletedRun.mockRejectedValue(new Error('already gone'))
    const warning = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    await controller.clearCurrentChat()

    expect(finalizeDeletedChat).toHaveBeenCalledWith('a', true)
    expect(cancelDeletedRun).toHaveBeenCalledWith('a')
    expect(reportClearError).not.toHaveBeenCalled()
    expect(window.location.hash).toBe('#chat')
    expect(warning).toHaveBeenCalledOnce()
    warning.mockRestore()
  })
})
