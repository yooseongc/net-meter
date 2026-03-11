import { create } from 'zustand'
import {
  isTestConfig,
  MetricsSnapshot,
  RuntimeConfig,
  TestConfig,
  TestEventType,
  TestResult,
  TestState,
  api,
  connectEventStream,
  connectMetricsWs,
} from '../api/client'

const MAX_HISTORY = 600  // 10분 (1초 간격 스냅샷 기준)
const MAX_EVENTS = 100

// B-1: localStorage 기반 프로파일 관리
const PROFILES_KEY = 'net-meter-profiles'

function loadProfilesFromStorage(): TestConfig[] {
  try {
    const raw = localStorage.getItem(PROFILES_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter(isTestConfig)
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
  runtimeConfig: RuntimeConfig

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
  const d = new Date()
  const ts = [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, '0'))
    .join(':')
  return { id: ++eventCounter, ts, level, message }
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
    case 'ramp_down_started':
      return makeEntry('info', `Ramp-down started — ${ev.ramp_down_secs}s to stop`)
    case 'ramp_down_complete':
      return makeEntry('info', 'Ramp-down complete')
    case 'ns_setup_complete':
      return makeEntry('info', 'Network namespace setup complete')
    case 'ns_teardown_complete':
      return makeEntry('info', 'Network namespace teardown complete')
    case 'ext_port_setup_complete':
      return makeEntry('info', 'External port setup complete')
    case 'ext_port_teardown_complete':
      return makeEntry('info', 'External port teardown complete')
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
  runtimeConfig: {
    mode: 'loopback',
    upper_iface: 'veth-c0',
    lower_iface: 'veth-s0',
  },

  fetchStatus: async () => {
    try {
      const status = await api.status()
      const prevState = get().testState
      const newState = status.state

      set({
        testState: newState,
        activeProfile: status.config,
        elapsedSecs: status.elapsed_secs,
        runtimeConfig: status.runtime ?? {
          mode: 'loopback',
          upper_iface: 'veth-c0',
          lower_iface: 'veth-s0',
        },
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
          // shift() 대신 slice()로 교체: shift는 O(n) 원소 이동, slice는 단순 범위 복사
          const prev = s.snapshotHistory
          const newHistory = prev.length >= MAX_HISTORY
            ? prev.slice(1).concat(snap)
            : prev.concat(snap)
          return { latestSnapshot: snap, snapshotHistory: newHistory, wsConnected: true }
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
            // 최신 항목을 앞에 추가하면서 최대 크기 초과 시 마지막 항목을 제거
            // log.length 직접 설정 대신 slice로 크기를 미리 결정해 불필요한 할당 방지
            const prev = s.eventLog
            const newLog = prev.length >= MAX_EVENTS
              ? [entry, ...prev.slice(0, MAX_EVENTS - 1)]
              : [entry, ...prev]
            return { eventLog: newLog }
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
