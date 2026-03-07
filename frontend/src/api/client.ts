// API 타입 정의 및 fetch 유틸리티

export type TestType = 'cps' | 'bw' | 'cc'
export type Protocol = 'tcp' | 'http1' | 'http2'
export type HttpMethod = 'GET' | 'POST'
export type NetworkMode = 'loopback' | 'namespace' | 'external_port'
export type VlanProto = 'dot1_q' | 'dot1_ad'
export type TestState =
  | 'idle'
  | 'preparing'
  | 'ramping_up'
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
// Load (per-client 기준)
// ---------------------------------------------------------------------------

export interface LoadConfig {
  /** [CPS] 클라이언트 1개당 초당 연결 수. 전체 CPS = client_count × cps_per_client */
  cps_per_client?: number
  /** [CC/BW] 클라이언트 1개당 동시 연결 수. 전체 CC = client_count × cc_per_client */
  cc_per_client?: number
  max_inflight_per_client?: number
  connect_timeout_ms?: number
  response_timeout_ms?: number
  ramp_up_secs?: number
}

export interface Thresholds {
  min_cps?: number
  max_error_rate_pct?: number
  max_latency_p99_ms?: number
  auto_stop_on_fail?: boolean
}

// ---------------------------------------------------------------------------
// 클라이언트 IP 대역
// ---------------------------------------------------------------------------

export interface ClientNet {
  base_ip: string
  /** 클라이언트 IP 수 (= 워커 수). undefined이면 total_clients 기준 자동 계산 */
  count?: number
  prefix_len?: number
}

// ---------------------------------------------------------------------------
// 서버 엔드포인트
// ---------------------------------------------------------------------------

export interface ServerEndpoint {
  id: string
  ip?: string
  port: number
}

// ---------------------------------------------------------------------------
// VLAN 설정
// ---------------------------------------------------------------------------

export interface VlanConfig {
  outer_vid: number
  inner_vid?: number
  outer_proto?: VlanProto
}

// ---------------------------------------------------------------------------
// Association (구 PairConfig)
// ---------------------------------------------------------------------------

export interface Association {
  id: string
  name: string
  client_net: ClientNet
  server: ServerEndpoint
  protocol: Protocol
  payload: PayloadProfile
  tls?: boolean
  load?: LoadConfig
  vlan?: VlanConfig
}

// ---------------------------------------------------------------------------
// 네트워크 설정
// ---------------------------------------------------------------------------

export interface NsOptions {
  netns_prefix: string
}

export interface ExternalPortOptions {
  client_iface: string
  server_iface: string
  client_gateway?: string
  client_gateway_mac?: string
  server_gateway?: string
  server_gateway_mac?: string
  flush_iface_addrs?: boolean
  cleanup_addrs?: boolean
}

export interface NetworkConfig {
  mode: NetworkMode
  ns: NsOptions
  ext?: ExternalPortOptions
  tcp_quickack: boolean
}

// ---------------------------------------------------------------------------
// TestConfig
// ---------------------------------------------------------------------------

export interface TestConfig {
  id: string
  name: string
  test_type: TestType
  duration_secs: number
  /** 전체 가상 클라이언트 수. associations 간 균등 분배. 0이면 각 association의 count 사용 */
  total_clients: number
  default_load: LoadConfig
  associations: Association[]
  network: NetworkConfig
  thresholds?: Thresholds
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
  status_code_breakdown: Record<number, number>
  threshold_violations: string[]
  is_ramping_up: boolean
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

// ---------------------------------------------------------------------------
// SSE 이벤트 타입
// ---------------------------------------------------------------------------

export type TestEventType =
  | { type: 'test_started'; config_name: string; test_type: string; duration_secs: number }
  | { type: 'test_stopped'; reason: string }
  | { type: 'ramp_up_started'; ramp_up_secs: number }
  | { type: 'ramp_up_complete' }
  | { type: 'ns_setup_complete' }
  | { type: 'ns_teardown_complete' }
  | { type: 'threshold_violation'; violations: string[] }
  | { type: 'error'; message: string }

/// SSE 이벤트 스트림 구독 (EventSource)
export function connectEventStream(
  onEvent: (event: TestEventType) => void,
  onError?: () => void,
): EventSource {
  const es = new EventSource('/api/events/stream')
  es.onmessage = (ev) => {
    try {
      const event = JSON.parse(ev.data) as TestEventType
      onEvent(event)
    } catch {
      /* ignore */
    }
  }
  es.onerror = () => onError?.()
  return es
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
