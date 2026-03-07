// API 타입 정의 및 fetch 유틸리티

export type TestType = 'cps' | 'bw' | 'cc'
export type Protocol = 'tcp' | 'http1' | 'http2'
export type HttpMethod = 'GET' | 'POST'
export type TestState =
  | 'idle'
  | 'preparing'
  | 'running'
  | 'stopping'
  | 'completed'
  | 'failed'

// ---------------------------------------------------------------------------
// Payload
// ---------------------------------------------------------------------------

export interface TcpPayload {
  type: 'tcp'
  tx_bytes: number
  rx_bytes: number
}

export interface HttpPayload {
  type: 'http'
  method: HttpMethod
  path: string
  request_body_bytes?: number
  response_body_bytes?: number
  path_extra_bytes?: number
  h2_max_concurrent_streams?: number
}

export type PayloadProfile = TcpPayload | HttpPayload

// ---------------------------------------------------------------------------
// Load, Endpoint, Pair
// ---------------------------------------------------------------------------

export interface LoadConfig {
  target_cps?: number
  target_cc?: number
  max_inflight?: number
  connect_timeout_ms?: number
  response_timeout_ms?: number
}

export interface ClientEndpoint {
  id: string
  ip?: string
}

export interface ServerEndpoint {
  id: string
  ip?: string
  port: number
}

export interface PairConfig {
  id: string
  client: ClientEndpoint
  server: ServerEndpoint
  protocol: Protocol
  payload: PayloadProfile
  load?: LoadConfig
}

export interface NsConfig {
  use_namespace: boolean
  netns_prefix: string
  tcp_quickack: boolean
}

// ---------------------------------------------------------------------------
// TestConfig (replaces TestProfile)
// ---------------------------------------------------------------------------

export interface TestConfig {
  id: string
  name: string
  test_type: TestType
  duration_secs: number
  default_load: LoadConfig
  pairs: PairConfig[]
  ns_config: NsConfig
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

export interface HistogramBucket {
  le_ms: number   // +Inf = Infinity
  count: number
}

export interface PerProtocolSnapshot {
  connections_attempted: number
  connections_established: number
  connections_failed: number
  connections_timed_out: number
  active_connections: number
  bytes_tx_total: number
  bytes_rx_total: number
  requests_total: number
  responses_total: number
  status_2xx: number
  status_4xx: number
  status_5xx: number
  latency_mean_ms: number
  latency_p99_ms: number
}

export interface MetricsSnapshot {
  timestamp_secs: number
  connections_attempted: number
  connections_established: number
  connections_failed: number
  connections_timed_out: number
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
  connect_mean_ms: number
  connect_p99_ms: number
  ttfb_mean_ms: number
  ttfb_p99_ms: number
  server_requests: number
  server_bytes_tx: number
  latency_histogram: HistogramBucket[]
  by_protocol: Record<string, PerProtocolSnapshot>
}

// ---------------------------------------------------------------------------
// Status & Results
// ---------------------------------------------------------------------------

export interface TestStatus {
  state: TestState
  config: TestConfig | null
  elapsed_secs: number | null
}

export interface TestResult {
  id: string
  config: TestConfig
  started_at_secs: number
  ended_at_secs: number
  elapsed_secs: number
  final_snapshot: MetricsSnapshot
}

// ---------------------------------------------------------------------------
// Fetch utilities
// ---------------------------------------------------------------------------

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
  listProfiles: () => fetchJson<TestConfig[]>('/profiles'),
  createProfile: (config: TestConfig) =>
    fetchJson<TestConfig>('/profiles', {
      method: 'POST',
      body: JSON.stringify(config),
    }),
  deleteProfile: (id: string) =>
    fetch(`${BASE}/profiles/${id}`, { method: 'DELETE' }),
  listResults: () => fetchJson<TestResult[]>('/results'),
  deleteResult: (id: string) =>
    fetch(`${BASE}/results/${id}`, { method: 'DELETE' }),
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
