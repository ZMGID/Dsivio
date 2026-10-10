import { useSyncExternalStore } from 'react'
import { api } from '../../api/tauri'
import type { ScheduledTask, ScheduledTaskRun, ScheduledTasksChangedEvent } from '../../api/scheduledTaskContracts'

interface ScheduledTasksSnapshot {
  tasks: ScheduledTask[] | null
  error: string
  revision: number
  /** Latest observed live run per task; terminal events remove the matching run. */
  liveRuns: Map<string, ScheduledTaskRun>
}

let snapshot: ScheduledTasksSnapshot = { tasks: null, error: '', revision: 0, liveRuns: new Map() }
const subscribers = new Set<() => void>()
let stopListening: (() => void) | undefined
let generation = 0
let started = false
let loading = false
let refreshPending = false
let listenerReady = false
/** Startup history is read once per subscription, after the listener is active. */
let seededGeneration = -1
/** Events observed while startup history is still in flight. */
let seedOpen = false
const seedLive = new Map<string, ScheduledTaskRun>()
const seedTerminal = new Map<string, Set<string>>()

function publish(next: ScheduledTasksSnapshot) {
  snapshot = next
  subscribers.forEach(notify => notify())
}

function isLiveStatus(status: ScheduledTaskRun['status']) {
  return status === 'queued' || status === 'running'
}

/** When the run became the live observation: start for a running run, creation for a queued one. */
function observedAt(run: ScheduledTaskRun) {
  return run.status === 'running' ? run.startedAt ?? run.createdAt : run.createdAt
}

function latestLiveRun(runs: readonly ScheduledTaskRun[]) {
  let best: ScheduledTaskRun | undefined
  let bestAt = -Infinity
  runs.forEach(run => {
    if (!isLiveStatus(run.status)) return
    const at = observedAt(run)
    if (!best || at > bestAt) {
      best = run
      bestAt = at
    }
  })
  return best
}

function noteSeedEvent(taskId: string, run: ScheduledTaskRun) {
  if (!seedOpen) return
  if (isLiveStatus(run.status)) {
    seedLive.set(taskId, run)
    return
  }
  if (seedLive.get(taskId)?.id === run.id) seedLive.delete(taskId)
  const blocked = seedTerminal.get(taskId) ?? new Set<string>()
  blocked.add(run.id)
  seedTerminal.set(taskId, blocked)
}

function resetSeed() {
  seedOpen = false
  seedLive.clear()
  seedTerminal.clear()
}

function mergeSeed(tasks: ScheduledTask[], histories: ScheduledTaskRun[][]) {
  const liveRuns = new Map<string, ScheduledTaskRun>()
  tasks.forEach((task, index) => {
    const guarded = seedLive.get(task.id)
    if (guarded) {
      liveRuns.set(task.id, guarded)
      return
    }
    const blocked = seedTerminal.get(task.id)
    const picked = latestLiveRun(histories[index].filter(run => !blocked?.has(run.id)))
    if (picked) liveRuns.set(task.id, picked)
  })
  for (const [taskId, run] of seedLive) {
    if (!liveRuns.has(taskId)) liveRuns.set(taskId, run)
  }
  return liveRuns
}

async function loadTasks(ownerGeneration: number) {
  if (ownerGeneration !== generation) return
  if (!listenerReady) { refreshPending = true; return }
  if (loading) { refreshPending = true; return }
  loading = true
  let listed: ScheduledTask[] | null = null
  try {
    listed = await api.scheduledTasksList()
    if (ownerGeneration !== generation) return
    if (seededGeneration !== ownerGeneration) {
      const histories = await Promise.all(listed.map(task => api.scheduledTaskRuns(task.id)))
      if (ownerGeneration !== generation) return
      const liveRuns = mergeSeed(listed, histories)
      seededGeneration = ownerGeneration
      resetSeed()
      publish({ ...snapshot, tasks: listed, error: '', liveRuns })
    } else {
      const taskIds = new Set(listed.map(task => task.id))
      const liveRuns = new Map([...snapshot.liveRuns].filter(([taskId]) => taskIds.has(taskId)))
      publish({ ...snapshot, tasks: listed, error: '', liveRuns })
    }
  } catch (error) {
    if (ownerGeneration !== generation) return
    publish(listed ? { ...snapshot, tasks: listed, error: String(error) } : { ...snapshot, error: String(error) })
  } finally {
    if (ownerGeneration === generation) {
      loading = false
      if (refreshPending) {
        refreshPending = false
        void loadTasks(ownerGeneration)
      }
    }
  }
}

function refresh() {
  publish({ ...snapshot, revision: snapshot.revision + 1 })
  if (started) void loadTasks(generation)
}

function onChanged({ taskId, run }: ScheduledTasksChangedEvent) {
  if (run) noteSeedEvent(taskId, run)
  const liveRuns = new Map(snapshot.liveRuns)
  if (run && isLiveStatus(run.status)) liveRuns.set(taskId, run)
  else if (run && liveRuns.get(taskId)?.id === run.id) liveRuns.delete(taskId)
  publish({ ...snapshot, liveRuns, revision: snapshot.revision + 1 })
  void loadTasks(generation)
}

function subscribe(notify: () => void) {
  subscribers.add(notify)
  if (!started) {
    started = true
    const ownerGeneration = ++generation
    listenerReady = false
    seededGeneration = -1
    resetSeed()
    seedOpen = true
    // Register before reading so a change during startup is kept. The list and
    // run snapshot are read only after listening is active; events in that
    // window supersede a late history response.
    void api.onScheduledTasksChanged(event => {
      if (ownerGeneration === generation) onChanged(event)
    }).then(dispose => {
      if (ownerGeneration !== generation) dispose()
      else {
        stopListening = dispose
        listenerReady = true
        refreshPending = false
        void loadTasks(ownerGeneration)
      }
    }).catch(error => {
      if (ownerGeneration === generation) publish({ ...snapshot, error: String(error) })
    })
  }
  return () => {
    subscribers.delete(notify)
    // React StrictMode's immediate re-subscription shares the same request/listener.
    queueMicrotask(() => {
      if (subscribers.size || !started) return
      started = false
      generation += 1
      stopListening?.()
      stopListening = undefined
      loading = false
      refreshPending = false
      listenerReady = false
      seededGeneration = -1
      resetSeed()
      snapshot = { ...snapshot, liveRuns: new Map() }
    })
  }
}

const getSnapshot = () => snapshot

/** Sidebar and Tasks page share one list request and one live-event subscription. */
export function useScheduledTasks() {
  return { ...useSyncExternalStore(subscribe, getSnapshot, getSnapshot), refresh }
}
