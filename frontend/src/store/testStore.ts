import { create } from 'zustand'
import {
  MetricsSnapshot,
  TestProfile,
  TestState,
  api,
  connectMetricsWs,
} from '../api/client'

const MAX_HISTORY = 120 // 최대 120초 분량 히스토리

interface TestStore {
  // 상태
  testState: TestState
  activeProfile: TestProfile | null
  latestSnapshot: MetricsSnapshot | null
  snapshotHistory: MetricsSnapshot[]
  savedProfiles: TestProfile[]
  wsConnected: boolean
  error: string | null

  // 액션
  fetchStatus: () => Promise<void>
  startTest: (profile: TestProfile) => Promise<void>
  stopTest: () => Promise<void>
  fetchProfiles: () => Promise<void>
  saveProfile: (profile: TestProfile) => Promise<void>
  deleteProfile: (id: string) => Promise<void>
  connectWs: () => void
  disconnectWs: () => void
}

let wsInstance: WebSocket | null = null

export const useTestStore = create<TestStore>((set, get) => ({
  testState: 'idle',
  activeProfile: null,
  latestSnapshot: null,
  snapshotHistory: [],
  savedProfiles: [],
  wsConnected: false,
  error: null,

  fetchStatus: async () => {
    try {
      const status = await api.status()
      set({
        testState: status.state,
        activeProfile: status.profile,
        error: null,
      })
    } catch (e) {
      set({ error: String(e) })
    }
  },

  startTest: async (profile) => {
    try {
      await api.startTest(profile)
      set({ testState: 'preparing', activeProfile: profile, error: null })
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

  saveProfile: async (profile) => {
    try {
      const saved = await api.createProfile(profile)
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

  connectWs: () => {
    if (wsInstance) return
    wsInstance = connectMetricsWs(
      (snap) => {
        set((s) => {
          const history = [...s.snapshotHistory, snap]
          if (history.length > MAX_HISTORY) history.shift()
          return { latestSnapshot: snap, snapshotHistory: history, wsConnected: true }
        })
        // 메트릭 수신 시 상태도 갱신
        get().fetchStatus()
      },
      () => {
        wsInstance = null
        set({ wsConnected: false })
        // 3초 후 재연결
        setTimeout(() => get().connectWs(), 3000)
      },
    )
  },

  disconnectWs: () => {
    wsInstance?.close()
    wsInstance = null
    set({ wsConnected: false })
  },
}))
