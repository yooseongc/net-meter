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

const MAX_HISTORY = 300  // 최대 5분 분량 히스토리
const MAX_EVENTS = 100   // 최대 이벤트 로그 항목 수

export interface EventLogEntry {
  id: number
  ts: string           // 로컬 시간 문자열
  level: 'info' | 'warn' | 'error'
  message: string
}

interface TestStore {
  // 상태
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

  // 액션
  fetchStatus: () => Promise<void>
  startTest: (config: TestConfig) => Promise<void>
  stopTest: () => Promise<void>
  fetchProfiles: () => Promise<void>
  saveProfile: (config: TestConfig) => Promise<void>
  deleteProfile: (id: string) => Promise<void>
  fetchResults: () => Promise<void>
  deleteResult: (id: string) => Promise<void>
  connectWs: () => void
  disconnectWs: () => void
  clearEventLog: () => void
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
  savedProfiles: [],
  testResults: [],
  wsConnected: false,
  error: null,
  eventLog: [],

  fetchStatus: async () => {
    try {
      const status = await api.status()
      set({
        testState: status.state,
        activeProfile: status.config,
        elapsedSecs: status.elapsed_secs,
        error: null,
      })
    } catch (e) {
      set({ error: String(e) })
    }
  },

  startTest: async (config) => {
    try {
      await api.startTest(config)
      set({ testState: 'preparing', activeProfile: config, elapsedSecs: 0, error: null })
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

  fetchProfiles: async () => {
    try {
      const profiles = await api.listProfiles()
      set({ savedProfiles: profiles, error: null })
    } catch (e) {
      set({ error: String(e) })
    }
  },

  saveProfile: async (config) => {
    try {
      const saved = await api.createProfile(config)
      set((s) => ({
        savedProfiles: [...s.savedProfiles.filter((p) => p.id !== saved.id), saved],
        error: null,
      }))
    } catch (e) {
      set({ error: String(e) })
    }
  },

  deleteProfile: async (id) => {
    try {
      await api.deleteProfile(id)
      set((s) => ({
        savedProfiles: s.savedProfiles.filter((p) => p.id !== id),
        error: null,
      }))
    } catch (e) {
      set({ error: String(e) })
    }
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
      set((s) => ({
        testResults: s.testResults.filter((r) => r.id !== id),
      }))
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
        get().fetchStatus()
      },
      () => {
        wsInstance = null
        set({ wsConnected: false })
        setTimeout(() => get().connectWs(), 3000)
      },
    )

    // SSE 이벤트 스트림 연결
    if (!esInstance) {
      esInstance = connectEventStream(
        (ev) => {
          const entry = eventToEntry(ev)
          set((s) => {
            const log = [entry, ...s.eventLog]
            if (log.length > MAX_EVENTS) log.length = MAX_EVENTS
            return { eventLog: log }
          })
          // 시험 상태 변경 이벤트 시 상태 동기화
          if (ev.type === 'test_stopped' || ev.type === 'ramp_up_complete') {
            get().fetchStatus()
          }
          // 시험 종료 시 결과 자동 갱신 (서버가 저장 완료할 시간을 줌)
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
}))
