import { create } from 'zustand'
import {
  MetricsSnapshot,
  TestConfig,
  TestEventType,
  TestResult,
  TestState,
  api,
  connectEventStream,
  connectMetricsWs,
} from '../api/client'

const MAX_HISTORY = 300
const MAX_EVENTS = 100

// B-1: localStorage 기반 프로파일 관리
const PROFILES_KEY = 'net-meter-profiles'

function loadProfilesFromStorage(): TestConfig[] {
  try {
    const raw = localStorage.getItem(PROFILES_KEY)
    return raw ? (JSON.parse(raw) as TestConfig[]) : []
  } catch {
    return []
  }
}

function saveProfilesToStorage(profiles: TestConfig[]) {
  localStorage.setItem(PROFILES_KEY, JSON.stringify(profiles))
}

// C-1: active 상태 판별
const ACTIVE_STATES: TestState[] = ['preparing', 'ramping_up', 'running', 'stopping']

export interface EventLogEntry {
  id: number
  ts: string
  level: 'info' | 'warn' | 'error'
  message: string
}

interface TestStore {
  testState: TestState
  activeProfile: TestConfig | null
  elapsedSecs: number | null
  latestSnapshot: MetricsSnapshot | null
  snapshotHistory: MetricsSnapshot[]
  savedProfiles: TestConfig[]
  testResults: TestResult[]
  wsConnected: boolean
  error: string | null
  eventLog: EventLogEntry[]
  draftConfig: TestConfig | null

  fetchStatus: () => Promise<void>
  startTest: (config: TestConfig) => Promise<void>
  stopTest: () => Promise<void>
  fetchProfiles: () => void           // B-1: 동기 (localStorage)
  saveProfile: (config: TestConfig) => void
  deleteProfile: (id: string) => void
  fetchResults: () => Promise<void>
  deleteResult: (id: string) => Promise<void>
  connectWs: () => void
  disconnectWs: () => void
  clearEventLog: () => void
  setDraftConfig: (c: TestConfig | null) => void
}

let wsInstance: WebSocket | null = null
let esInstance: EventSource | null = null
let eventCounter = 0

function makeEntry(level: EventLogEntry['level'], message: string): EventLogEntry {
  return { id: ++eventCounter, ts: new Date().toLocaleTimeString(), level, message }
}

function eventToEntry(ev: TestEventType): EventLogEntry {
  switch (ev.type) {
    case 'test_started':
      return makeEntry('info', `Test started: "${ev.config_name}" [${ev.test_type.toUpperCase()}${ev.duration_secs > 0 ? ` ${ev.duration_secs}s` : ''}]`)
    case 'test_stopped':
      return makeEntry('info', `Test stopped (${ev.reason})`)
    case 'ramp_up_started':
      return makeEntry('info', `Ramp-up started — ${ev.ramp_up_secs}s to full speed`)
    case 'ramp_up_complete':
      return makeEntry('info', 'Ramp-up complete — running at full speed')
    case 'ns_setup_complete':
      return makeEntry('info', 'Network namespace setup complete')
    case 'ns_teardown_complete':
      return makeEntry('info', 'Network namespace teardown complete')
    case 'threshold_violation':
      return makeEntry('warn', `Threshold violation: ${ev.violations.join('; ')}`)
    case 'error':
      return makeEntry('error', `Error: ${ev.message}`)
  }
}

export const useTestStore = create<TestStore>((set, get) => ({
  testState: 'idle',
  activeProfile: null,
  elapsedSecs: null,
  latestSnapshot: null,
  snapshotHistory: [],
  savedProfiles: loadProfilesFromStorage(),  // B-1: localStorage에서 초기화
  testResults: [],
  wsConnected: false,
  error: null,
  eventLog: [],
  draftConfig: null,

  fetchStatus: async () => {
    try {
      const status = await api.status()
      const prevState = get().testState
      const newState = status.state

      set({
        testState: newState,
        activeProfile: status.config,
        elapsedSecs: status.elapsed_secs,
        error: null,
      })

      // C-1: failed 진입 시 — 3초 후 히스토리 정리
      if (
        (newState === 'failed') &&
        prevState !== 'failed' && prevState !== 'idle'
      ) {
        setTimeout(() => {
          if (['failed', 'idle'].includes(get().testState)) {
            set({ snapshotHistory: [] })
          }
        }, 3000)
      }
    } catch (e) {
      set({ error: String(e) })
    }
  },

  startTest: async (config) => {
    try {
      await api.startTest(config)
      set({
        testState: 'preparing',
        activeProfile: config,
        elapsedSecs: 0,
        snapshotHistory: [],
        error: null,
      })
    } catch (e) {
      set({ error: String(e) })
    }
  },

  stopTest: async () => {
    try {
      await api.stopTest()
      set({ testState: 'stopping', error: null })
    } catch (e) {
      set({ error: String(e) })
    }
  },

  // B-1: localStorage 기반 프로파일 — API 호출 없음
  fetchProfiles: () => {
    set({ savedProfiles: loadProfilesFromStorage() })
  },

  saveProfile: (config) => {
    const current = get().savedProfiles
    const updated = [...current.filter((p) => p.id !== config.id), config]
    saveProfilesToStorage(updated)
    set({ savedProfiles: updated })
  },

  deleteProfile: (id) => {
    const updated = get().savedProfiles.filter((p) => p.id !== id)
    saveProfilesToStorage(updated)
    set({ savedProfiles: updated })
  },

  fetchResults: async () => {
    try {
      const results = await api.listResults()
      set({ testResults: results, error: null })
    } catch (e) {
      set({ error: String(e) })
    }
  },

  deleteResult: async (id) => {
    try {
      await api.deleteResult(id)
      set((s) => ({ testResults: s.testResults.filter((r) => r.id !== id) }))
    } catch (e) {
      set({ error: String(e) })
    }
  },

  connectWs: () => {
    if (wsInstance) return
    wsInstance = connectMetricsWs(
      (snap) => {
        set((s) => {
          const history = [...s.snapshotHistory, snap]
          if (history.length > MAX_HISTORY) history.shift()
          return { latestSnapshot: snap, snapshotHistory: history, wsConnected: true }
        })
        // C-1: active 상태일 때만 fetchStatus 폴링
        if (ACTIVE_STATES.includes(get().testState)) {
          get().fetchStatus()
        }
      },
      () => {
        wsInstance = null
        set({ wsConnected: false })
        setTimeout(() => get().connectWs(), 3000)
      },
    )

    if (!esInstance) {
      esInstance = connectEventStream(
        (ev) => {
          const entry = eventToEntry(ev)
          set((s) => {
            const log = [entry, ...s.eventLog]
            if (log.length > MAX_EVENTS) log.length = MAX_EVENTS
            return { eventLog: log }
          })
          // C-1: 상태 전환 이벤트 시 동기화
          if (
            ev.type === 'test_stopped' ||
            ev.type === 'ramp_up_complete' ||
            ev.type === 'error'
          ) {
            get().fetchStatus()
          }
          if (ev.type === 'test_stopped') {
            setTimeout(() => get().fetchResults(), 600)
          }
        },
        () => {
          esInstance = null
        },
      )
    }
  },

  disconnectWs: () => {
    wsInstance?.close()
    wsInstance = null
    esInstance?.close()
    esInstance = null
    set({ wsConnected: false })
  },

  clearEventLog: () => set({ eventLog: [] }),

  setDraftConfig: (c) => set({ draftConfig: c }),
}))
