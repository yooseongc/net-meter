import {
  LineChart,
  Line,
  BarChart,
  Bar,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  ReferenceLine,
  Legend,
} from 'recharts'
import { useTestStore } from '../store/testStore'
import { MetricsSnapshot } from '../api/client'

// ─── 유틸 ────────────────────────────────────────────────────────────────────

function fmtBytes(bps: number): string {
  if (bps >= 1e9) return `${(bps / 1e9).toFixed(2)} GB/s`
  if (bps >= 1e6) return `${(bps / 1e6).toFixed(2)} MB/s`
  if (bps >= 1e3) return `${(bps / 1e3).toFixed(1)} KB/s`
  return `${bps.toFixed(0)} B/s`
}

function successRate(snap: MetricsSnapshot): number {
  return snap.responses_total > 0 ? (snap.status_2xx / snap.responses_total) * 100 : 0
}

function errorRate(snap: MetricsSnapshot): number {
  return snap.responses_total > 0
    ? ((snap.status_4xx + snap.status_5xx + snap.status_other) / snap.responses_total) * 100
    : 0
}

// ─── 구성요소 ─────────────────────────────────────────────────────────────────

// 목표값 vs 실측값 큰 카드
function TargetCard({
  label,
  actual,
  target,
  unit,
  color,
  format = (v) => v.toFixed(1),
}: {
  label: string
  actual: number
  target?: number
  unit: string
  color: string
  format?: (v: number) => string
}) {
  const pct = target && target > 0 ? (actual / target) * 100 : null
  const pctColor = pct == null ? color : pct >= 95 ? '#3fb950' : pct >= 70 ? '#d29922' : '#f85149'

  return (
    <div className="card">
      <div className="card-title">{label}</div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
        <span style={{ fontSize: 32, fontWeight: 700, color, lineHeight: 1 }}>
          {format(actual)}
        </span>
        <span style={{ fontSize: 14, color: '#8b949e' }}>{unit}</span>
      </div>
      {target != null && (
        <div style={{ marginTop: 6 }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11, marginBottom: 3 }}>
            <span style={{ color: '#8b949e' }}>Target: {format(target)}</span>
            {pct != null && (
              <span style={{ color: pctColor, fontWeight: 700 }}>{pct.toFixed(0)}%</span>
            )}
          </div>
          <div style={{ height: 3, background: '#21262d', borderRadius: 2, overflow: 'hidden' }}>
            <div
              style={{
                height: '100%',
                width: `${Math.min(100, pct ?? 0)}%`,
                background: pctColor,
                borderRadius: 2,
                transition: 'width 0.5s',
              }}
            />
          </div>
        </div>
      )}
    </div>
  )
}

// 단순 숫자 카드
function StatCard({
  label,
  value,
  unit,
  color = '#e6edf3',
  small = false,
}: {
  label: string
  value: string
  unit?: string
  color?: string
  small?: boolean
}) {
  return (
    <div className="card">
      <div className="card-title">{label}</div>
      <div style={{ fontSize: small ? 20 : 26, fontWeight: 700, color }}>
        {value}
        {unit && <span style={{ fontSize: 12, color: '#8b949e', marginLeft: 4 }}>{unit}</span>}
      </div>
    </div>
  )
}

// 차트 공통 props
const chartStyle = { background: '#161b22', border: '1px solid #30363d', borderRadius: 8, padding: 16 }
const tooltipStyle = { background: '#0d1117', border: '1px solid #30363d', fontSize: 12 }
const axisStyle = { fontSize: 11, fill: '#8b949e' }
const gridStyle = { strokeDasharray: '3 3', stroke: '#21262d' }

// Latency histogram
function LatencyHistogram({ snap }: { snap: MetricsSnapshot }) {
  if (!snap.latency_histogram || snap.latency_histogram.length < 2) return null

  // 누적 → 비누적(구간별 카운트) 변환
  const buckets = snap.latency_histogram
  const barData = buckets
    .slice(0, -1) // +Inf 제외
    .map((b, i) => ({
      le: b.le_ms >= 1000 ? `${b.le_ms / 1000}s` : `${b.le_ms}ms`,
      count: i === 0 ? b.count : b.count - buckets[i - 1].count,
    }))
    .filter((d) => d.count > 0)

  if (barData.length === 0) return null

  const p50 = snap.latency_p50_ms
  const p95 = snap.latency_p95_ms
  const p99 = snap.latency_p99_ms

  return (
    <div style={chartStyle}>
      <div className="card-title" style={{ marginBottom: 8 }}>Latency Distribution</div>
      <div style={{ display: 'flex', gap: 16, marginBottom: 8 }}>
        {[
          { label: 'p50', value: p50, color: '#3fb950' },
          { label: 'p95', value: p95, color: '#d29922' },
          { label: 'p99', value: p99, color: '#f85149' },
          { label: 'max', value: snap.latency_max_ms, color: '#8b949e' },
        ].map(({ label, value, color }) => (
          <div key={label} style={{ fontSize: 12 }}>
            <span style={{ color: '#8b949e' }}>{label}: </span>
            <span style={{ color, fontWeight: 700 }}>{value.toFixed(2)}ms</span>
          </div>
        ))}
      </div>
      <ResponsiveContainer width="100%" height={140}>
        <BarChart data={barData} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <CartesianGrid {...gridStyle} />
          <XAxis dataKey="le" tick={axisStyle} />
          <YAxis tick={axisStyle} />
          <Tooltip contentStyle={tooltipStyle} />
          <Bar dataKey="count" fill="#58a6ff" name="Requests" radius={[2, 2, 0, 0]} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  )
}

// 에러 breakdown
function ErrorBreakdown({ snap }: { snap: MetricsSnapshot }) {
  const items = [
    { label: '2xx', count: snap.status_2xx, color: '#3fb950' },
    { label: '4xx', count: snap.status_4xx, color: '#d29922' },
    { label: '5xx', count: snap.status_5xx, color: '#f85149' },
    { label: 'other', count: snap.status_other, color: '#8b949e' },
    { label: 'failed', count: snap.connections_failed, color: '#f85149' },
    { label: 'timeout', count: snap.connections_timed_out ?? 0, color: '#d29922' },
  ]
  const total = snap.responses_total + snap.connections_failed

  return (
    <div className="card">
      <div className="card-title">Error Breakdown</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        {items.map(({ label, count, color }) => {
          const pct = total > 0 ? ((count / total) * 100).toFixed(1) : '0.0'
          return (
            <div key={label}>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12, marginBottom: 2 }}>
                <span style={{ color }}>{label}</span>
                <span style={{ color: '#8b949e' }}>
                  {count.toLocaleString()} <span style={{ color: '#484f58' }}>({pct}%)</span>
                </span>
              </div>
              <div style={{ height: 3, background: '#21262d', borderRadius: 2 }}>
                <div
                  style={{
                    height: '100%',
                    width: `${pct}%`,
                    background: color,
                    borderRadius: 2,
                    transition: 'width 0.3s',
                  }}
                />
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ─── 메인 컴포넌트 ────────────────────────────────────────────────────────────

export default function MetricsPanel() {
  const { latestSnapshot: snap, snapshotHistory: history, activeProfile: profile } =
    useTestStore()

  if (!snap) {
    return (
      <div className="card" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', minHeight: 300 }}>
        <span style={{ color: '#8b949e', fontSize: 14 }}>Waiting for metrics…</span>
      </div>
    )
  }

  const chartData = history.map((s, i) => ({
    t: i,
    cps: +s.cps.toFixed(2),
    rps: +s.rps.toFixed(2),
    active: s.active_connections,
    bwTx: +(s.bytes_tx_per_sec / 1024).toFixed(2),
    bwRx: +(s.bytes_rx_per_sec / 1024).toFixed(2),
    errRate: +errorRate(s).toFixed(2),
    latMean: +s.latency_mean_ms.toFixed(2),
    latP99: +s.latency_p99_ms.toFixed(2),
  }))

  const targetCps = profile?.test_type === 'cps' ? profile.target_cps : undefined
  const targetCc = profile?.target_cc
  const succ = successRate(snap)

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>

      {/* 핵심 목표 지표 카드 */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 10 }}>
        <TargetCard
          label="CPS"
          actual={snap.cps}
          target={targetCps}
          unit="/s"
          color="#3fb950"
        />
        <TargetCard
          label="Active Conn"
          actual={snap.active_connections}
          target={targetCc}
          unit=""
          color="#d29922"
          format={(v) => v.toFixed(0)}
        />
        <StatCard
          label="RPS"
          value={snap.rps.toFixed(1)}
          unit="/s"
          color="#58a6ff"
        />
        <StatCard
          label="Success Rate"
          value={succ.toFixed(1)}
          unit="%"
          color={succ >= 99 ? '#3fb950' : succ >= 90 ? '#d29922' : '#f85149'}
        />
      </div>

      {/* 보조 지표 카드 */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 10 }}>
        <StatCard label="Latency p50" value={snap.latency_p50_ms.toFixed(2)} unit="ms" color="#bc8cff" small />
        <StatCard label="Latency p99" value={snap.latency_p99_ms.toFixed(2)} unit="ms" color="#f85149" small />
        <StatCard label="TX" value={fmtBytes(snap.bytes_tx_per_sec)} color="#d29922" small />
        <StatCard label="RX" value={fmtBytes(snap.bytes_rx_per_sec)} color="#bc8cff" small />
      </div>

      {/* 연결 누계 */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 10 }}>
        <StatCard label="Attempted" value={snap.connections_attempted.toLocaleString()} color="#8b949e" small />
        <StatCard label="Established" value={snap.connections_established.toLocaleString()} color="#3fb950" small />
        <StatCard label="Failed" value={snap.connections_failed.toLocaleString()} color="#f85149" small />
        <StatCard label="Responses" value={snap.responses_total.toLocaleString()} color="#58a6ff" small />
      </div>

      {/* CPS / RPS 차트 (목표선 포함) */}
      {chartData.length > 1 && (
        <div style={chartStyle}>
          <div className="card-title" style={{ marginBottom: 8 }}>Connections & Requests / sec</div>
          <ResponsiveContainer width="100%" height={200}>
            <LineChart data={chartData} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
              <CartesianGrid {...gridStyle} />
              <XAxis dataKey="t" tick={axisStyle} />
              <YAxis tick={axisStyle} />
              <Tooltip contentStyle={tooltipStyle} />
              <Legend wrapperStyle={{ fontSize: 12 }} />
              {targetCps != null && (
                <ReferenceLine
                  y={targetCps}
                  stroke="#3fb950"
                  strokeDasharray="6 3"
                  label={{ value: `target ${targetCps}`, fill: '#3fb950', fontSize: 10 }}
                />
              )}
              <Line type="monotone" dataKey="cps" stroke="#3fb950" dot={false} name="CPS" strokeWidth={2} />
              <Line type="monotone" dataKey="rps" stroke="#58a6ff" dot={false} name="RPS" strokeWidth={1.5} />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}

      {/* Active Connections 차트 */}
      {chartData.length > 1 && (
        <div style={chartStyle}>
          <div className="card-title" style={{ marginBottom: 8 }}>Active Connections</div>
          <ResponsiveContainer width="100%" height={150}>
            <AreaChart data={chartData} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
              <defs>
                <linearGradient id="activeGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#d29922" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="#d29922" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid {...gridStyle} />
              <XAxis dataKey="t" tick={axisStyle} />
              <YAxis tick={axisStyle} />
              <Tooltip contentStyle={tooltipStyle} />
              {targetCc != null && (
                <ReferenceLine
                  y={targetCc}
                  stroke="#d29922"
                  strokeDasharray="6 3"
                  label={{ value: `target ${targetCc}`, fill: '#d29922', fontSize: 10 }}
                />
              )}
              <Area
                type="monotone"
                dataKey="active"
                stroke="#d29922"
                fill="url(#activeGrad)"
                dot={false}
                name="Active Conn"
                strokeWidth={2}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}

      {/* Bandwidth (Stacked Area) */}
      {chartData.length > 1 && (
        <div style={chartStyle}>
          <div className="card-title" style={{ marginBottom: 8 }}>Bandwidth (KB/s)</div>
          <ResponsiveContainer width="100%" height={150}>
            <AreaChart data={chartData} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
              <defs>
                <linearGradient id="txGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#d29922" stopOpacity={0.4} />
                  <stop offset="95%" stopColor="#d29922" stopOpacity={0} />
                </linearGradient>
                <linearGradient id="rxGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#bc8cff" stopOpacity={0.4} />
                  <stop offset="95%" stopColor="#bc8cff" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid {...gridStyle} />
              <XAxis dataKey="t" tick={axisStyle} />
              <YAxis tick={axisStyle} />
              <Tooltip contentStyle={tooltipStyle} />
              <Legend wrapperStyle={{ fontSize: 12 }} />
              <Area type="monotone" dataKey="bwTx" stroke="#d29922" fill="url(#txGrad)" dot={false} name="TX KB/s" strokeWidth={1.5} />
              <Area type="monotone" dataKey="bwRx" stroke="#bc8cff" fill="url(#rxGrad)" dot={false} name="RX KB/s" strokeWidth={1.5} />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}

      {/* Latency 시계열 */}
      {chartData.length > 1 && (
        <div style={chartStyle}>
          <div className="card-title" style={{ marginBottom: 8 }}>Latency (ms)</div>
          <ResponsiveContainer width="100%" height={150}>
            <LineChart data={chartData} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
              <CartesianGrid {...gridStyle} />
              <XAxis dataKey="t" tick={axisStyle} />
              <YAxis tick={axisStyle} />
              <Tooltip contentStyle={tooltipStyle} />
              <Legend wrapperStyle={{ fontSize: 12 }} />
              <Line type="monotone" dataKey="latMean" stroke="#58a6ff" dot={false} name="mean" strokeWidth={1.5} />
              <Line type="monotone" dataKey="latP99" stroke="#f85149" dot={false} name="p99" strokeWidth={1.5} />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}

      {/* 하단 2단: Latency Histogram + Error Breakdown */}
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
        <LatencyHistogram snap={snap} />
        <ErrorBreakdown snap={snap} />
      </div>

    </div>
  )
}
