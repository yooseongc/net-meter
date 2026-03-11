export type {
  Association,
  ClientDef,
  HistogramBucket,
  HttpMethod,
  HttpPayload,
  LoadConfig,
  MetricsSnapshot,
  NetworkMode,
  PayloadProfile,
  PerProtocolSnapshot,
  Protocol,
  RuntimeConfig,
  ServerDef,
  TcpOptions,
  TcpPayload,
  TestConfig,
  TestEventType,
  TestResult,
  TestState,
  TestStatus,
  TestType,
  Thresholds,
  VlanConfig,
  VlanProto,
} from './generated'

import type {
  MetricsSnapshot,
  TestConfig,
  TestEventType,
  TestResult,
  TestStatus,
} from './generated'

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

export function isTestConfig(value: unknown): value is TestConfig {
  if (!isRecord(value)) return false
  return (
    typeof value.id === 'string' &&
    typeof value.name === 'string' &&
    (value.test_type === 'cps' || value.test_type === 'cc' || value.test_type === 'bw') &&
    typeof value.duration_secs === 'number' &&
    isRecord(value.default_load) &&
    Array.isArray(value.clients) &&
    Array.isArray(value.servers) &&
    Array.isArray(value.associations) &&
    (!('tcp_options' in value) || value.tcp_options === undefined || isRecord(value.tcp_options)) &&
    (!('thresholds' in value) || value.thresholds === undefined || isRecord(value.thresholds))
  )
}

const BASE = '/api'

async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(`${res.status} ${res.statusText}: ${text}`)
  }
  return res.json()
}

export const api = {
  health: () => fetchJson<{ status: string; version: string }>('/health'),
  status: () => fetchJson<TestStatus>('/status'),
  startTest: (config: TestConfig) =>
    fetchJson<{ status: string }>('/test/start', {
      method: 'POST',
      body: JSON.stringify(config),
    }),
  stopTest: () =>
    fetchJson<{ status: string }>('/test/stop', { method: 'POST' }),
  getMetrics: () => fetchJson<MetricsSnapshot>('/metrics'),
  listResults: () => fetchJson<TestResult[]>('/results'),
  deleteResult: async (id: string) => {
    const res = await fetch(`${BASE}/results/${id}`, { method: 'DELETE' })
    if (!res.ok) {
      const text = await res.text()
      throw new Error(`${res.status} ${res.statusText}: ${text}`)
    }
  },
}

export function connectEventStream(
  onEvent: (event: TestEventType) => void,
  onError?: () => void,
): EventSource {
  const es = new EventSource('/api/events/stream')
  es.onmessage = (ev) => {
    try {
      const event = JSON.parse(ev.data) as TestEventType
      onEvent(event)
    } catch (e) {
      console.warn('[net-meter] SSE event parse failed:', e, '| raw:', ev.data?.slice(0, 200))
    }
  }
  es.onerror = () => onError?.()
  return es
}

export function connectMetricsWs(
  onSnapshot: (snap: MetricsSnapshot) => void,
  onClose?: () => void,
): WebSocket {
  const wsScheme = window.location.protocol === 'https:' ? 'wss' : 'ws'
  const wsUrl = `${wsScheme}://${window.location.host}/api/metrics/ws`
  const ws = new WebSocket(wsUrl)
  ws.onmessage = (ev) => {
    try {
      const snap = JSON.parse(ev.data) as MetricsSnapshot
      onSnapshot(snap)
    } catch (e) {
      console.warn('[net-meter] WS metrics parse failed:', e, '| raw:', ev.data?.slice(0, 200))
    }
  }
  ws.onclose = () => onClose?.()
  return ws
}
