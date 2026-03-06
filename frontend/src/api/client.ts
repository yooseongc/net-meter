// API 타입 정의 및 fetch 유틸리티

export type TestType = 'cps' | 'bw' | 'cc'
export type Protocol = 'http1' | 'http2'
export type HttpMethod = 'GET' | 'POST'
export type TestState =
  | 'idle'
  | 'preparing'
  | 'running'
  | 'stopping'
  | 'completed'
  | 'failed'

export interface TestProfile {
  id: string
  name: string
  test_type: TestType
  protocol: Protocol
  target_host: string
  target_port: number
  duration_secs: number
  target_cps?: number
  target_cc?: number
  request_body_bytes?: number
  response_body_bytes?: number
  method: HttpMethod
  path: string
  connect_timeout_ms?: number
  response_timeout_ms?: number
  max_inflight?: number
  use_namespace?: boolean
  netns_prefix?: string
  num_clients?: number
  num_servers?: number
}

export interface MetricsSnapshot {
  timestamp_secs: number
  connections_attempted: number
  connections_established: number
  connections_failed: number
  active_connections: number
  requests_total: number
  responses_total: number
  status_2xx: number
  status_4xx: number
  status_5xx: number
  status_other: number
  bytes_tx_total: number
  bytes_rx_total: number
  cps: number
  rps: number
  bytes_tx_per_sec: number
  bytes_rx_per_sec: number
  latency_mean_ms: number
  latency_p50_ms: number
  latency_p95_ms: number
  latency_p99_ms: number
  latency_max_ms: number
}

export interface TestStatus {
  state: TestState
  profile: TestProfile | null
  elapsed_secs: number | null
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
  startTest: (profile: TestProfile) =>
    fetchJson<{ status: string }>('/test/start', {
      method: 'POST',
      body: JSON.stringify(profile),
    }),
  stopTest: () =>
    fetchJson<{ status: string }>('/test/stop', { method: 'POST' }),
  getMetrics: () => fetchJson<MetricsSnapshot>('/metrics'),
  listProfiles: () => fetchJson<TestProfile[]>('/profiles'),
  createProfile: (profile: TestProfile) =>
    fetchJson<TestProfile>('/profiles', {
      method: 'POST',
      body: JSON.stringify(profile),
    }),
  deleteProfile: (id: string) =>
    fetch(`${BASE}/profiles/${id}`, { method: 'DELETE' }),
}

/// WebSocket 연결로 실시간 메트릭 스트림 구독
export function connectMetricsWs(
  onSnapshot: (snap: MetricsSnapshot) => void,
  onClose?: () => void,
): WebSocket {
  const wsUrl = `ws://${window.location.host}/api/metrics/ws`
  const ws = new WebSocket(wsUrl)
  ws.onmessage = (ev) => {
    try {
      const snap = JSON.parse(ev.data) as MetricsSnapshot
      onSnapshot(snap)
    } catch {
      /* ignore parse errors */
    }
  }
  ws.onclose = () => onClose?.()
  return ws
}
