import {
  LineChart, Line, BarChart, Bar, AreaChart, Area,
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
  ReferenceLine, Legend,
} from 'recharts'
import { useTestStore } from '../store/testStore'
import { MetricsSnapshot, TestConfig } from '../api/client'
import { ChartKey } from './TestRunPanel'

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

// ─── 컴팩트 메트릭 테이블 ─────────────────────────────────────────────────────

function MetricCell({
  label, value, unit, sub, color = '#e6edf3', barPct,
}: {
  label: string; value: string; unit?: string; sub?: string
  color?: string; barPct?: number
}) {
  return (
    <div style={{
      background: '#0d1117', borderRadius: 6, padding: '8px 12px',
      display: 'flex', flexDirection: 'column', gap: 2,
    }}>
      <div style={{ fontSize: 10, color: '#484f58', textTransform: 'uppercase', letterSpacing: '0.06em' }}>
        {label}
      </div>
      <div style={{ fontSize: 20, fontWeight: 700, color, lineHeight: 1.2 }}>
        {value}
        {unit && <span style={{ fontSize: 11, color: '#8b949e', marginLeft: 3 }}>{unit}</span>}
      </div>
      {sub && <div style={{ fontSize: 11, color: '#8b949e' }}>{sub}</div>}
      {barPct != null && (
        <div style={{ height: 2, background: '#21262d', borderRadius: 1, overflow: 'hidden', marginTop: 2 }}>
          <div style={{
            height: '100%', width: `${Math.min(100, barPct)}%`,
            background: barPct >= 95 ? '#3fb950' : barPct >= 70 ? '#d29922' : '#f85149',
            borderRadius: 1, transition: 'width 0.5s',
          }} />
        </div>
      )}
    </div>
  )
}

function MetricsTable({ snap, profile }: { snap: MetricsSnapshot; profile: TestConfig | null }) {
  const succ = successRate(snap)
  const targetCps = profile?.test_type === 'cps' ? profile.default_load.cps_per_client : undefined
  const targetCc = profile?.default_load.cc_per_client

  const cpsPct = targetCps ? (snap.cps / targetCps) * 100 : undefined
  const ccPct = targetCc ? (snap.active_connections / targetCc) * 100 : undefined

  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 6 }}>
      <MetricCell
        label="CPS" value={snap.cps.toFixed(1)} unit="/s" color="#3fb950"
        sub={targetCps ? `target ${targetCps}` : undefined}
        barPct={cpsPct}
      />
      <MetricCell
        label="Active Conn" value={snap.active_connections.toFixed(0)} color="#d29922"
        sub={targetCc ? `target ${targetCc}` : undefined}
        barPct={ccPct}
      />
      <MetricCell label="RPS" value={snap.rps.toFixed(1)} unit="/s" color="#58a6ff" />
      <MetricCell
        label="Success" value={succ.toFixed(1)} unit="%"
        color={succ >= 99 ? '#3fb950' : succ >= 90 ? '#d29922' : '#f85149'}
      />
      <MetricCell label="Latency p50" value={snap.latency_p50_ms.toFixed(2)} unit="ms" color="#bc8cff" />
      <MetricCell label="Latency p99" value={snap.latency_p99_ms.toFixed(2)} unit="ms" color="#f85149" />
      <MetricCell label="TX" value={fmtBytes(snap.bytes_tx_per_sec)} color="#d29922" />
      <MetricCell label="RX" value={fmtBytes(snap.bytes_rx_per_sec)} color="#bc8cff" />
    </div>
  )
}

// ─── 차트 공통 ────────────────────────────────────────────────────────────────

const chartStyle = { background: '#161b22', border: '1px solid #30363d', borderRadius: 8, padding: 12 }
const tooltipStyle = { background: '#0d1117', border: '1px solid #30363d', fontSize: 12 }
const axisStyle = { fontSize: 11, fill: '#8b949e' }
const gridStyle = { strokeDasharray: '3 3', stroke: '#21262d' }

// ─── 개별 차트 컴포넌트 ────────────────────────────────────────────────────────

function CpsRpsChart({ data, targetCps }: {
  data: ReturnType<typeof buildChartData>; targetCps?: number
}) {
  return (
    <div style={chartStyle}>
      <div className="card-title" style={{ marginBottom: 8 }}>Connections & Requests / sec</div>
      <ResponsiveContainer width="100%" height={160}>
        <LineChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <CartesianGrid {...gridStyle} />
          <XAxis dataKey="t" tick={axisStyle} />
          <YAxis tick={axisStyle} />
          <Tooltip contentStyle={tooltipStyle} />
          <Legend wrapperStyle={{ fontSize: 12 }} />
          {targetCps != null && (
            <ReferenceLine y={targetCps} stroke="#3fb950" strokeDasharray="6 3"
              label={{ value: `target ${targetCps}`, fill: '#3fb950', fontSize: 10 }} />
          )}
          <Line type="monotone" dataKey="cps" stroke="#3fb950" dot={false} name="CPS" strokeWidth={2} />
          <Line type="monotone" dataKey="rps" stroke="#58a6ff" dot={false} name="RPS" strokeWidth={1.5} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  )
}

function ActiveConnChart({ data, targetCc }: {
  data: ReturnType<typeof buildChartData>; targetCc?: number
}) {
  return (
    <div style={chartStyle}>
      <div className="card-title" style={{ marginBottom: 8 }}>Active Connections</div>
      <ResponsiveContainer width="100%" height={130}>
        <AreaChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
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
            <ReferenceLine y={targetCc} stroke="#d29922" strokeDasharray="6 3"
              label={{ value: `target ${targetCc}`, fill: '#d29922', fontSize: 10 }} />
          )}
          <Area type="monotone" dataKey="active" stroke="#d29922" fill="url(#activeGrad)"
            dot={false} name="Active Conn" strokeWidth={2} />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  )
}

function BandwidthChart({ data }: { data: ReturnType<typeof buildChartData> }) {
  return (
    <div style={chartStyle}>
      <div className="card-title" style={{ marginBottom: 8 }}>Bandwidth (KB/s)</div>
      <ResponsiveContainer width="100%" height={130}>
        <AreaChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
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
  )
}

function LatencyChart({ data }: { data: ReturnType<typeof buildChartData> }) {
  return (
    <div style={chartStyle}>
      <div className="card-title" style={{ marginBottom: 8 }}>Latency (ms)</div>
      <ResponsiveContainer width="100%" height={130}>
        <LineChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
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
  )
}

function LatencyHistogram({ snap }: { snap: MetricsSnapshot }) {
  if (!snap.latency_histogram || snap.latency_histogram.length < 2) return null
  const buckets = snap.latency_histogram
  const barData = buckets
    .slice(0, -1)
    .map((b, i) => ({
      le: b.le_ms >= 1000 ? `${b.le_ms / 1000}s` : `${b.le_ms}ms`,
      count: i === 0 ? b.count : b.count - buckets[i - 1].count,
    }))
    .filter((d) => d.count > 0)
  if (barData.length === 0) return null

  return (
    <div style={chartStyle}>
      <div className="card-title" style={{ marginBottom: 6 }}>Latency Distribution</div>
      <div style={{ display: 'flex', gap: 14, marginBottom: 8 }}>
        {[
          { label: 'p50', value: snap.latency_p50_ms, color: '#3fb950' },
          { label: 'p95', value: snap.latency_p95_ms, color: '#d29922' },
          { label: 'p99', value: snap.latency_p99_ms, color: '#f85149' },
          { label: 'max', value: snap.latency_max_ms, color: '#8b949e' },
        ].map(({ label, value, color }) => (
          <div key={label} style={{ fontSize: 12 }}>
            <span style={{ color: '#8b949e' }}>{label}: </span>
            <span style={{ color, fontWeight: 700 }}>{value.toFixed(2)}ms</span>
          </div>
        ))}
      </div>
      <ResponsiveContainer width="100%" height={120}>
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

function ErrorBreakdown({ snap }: { snap: MetricsSnapshot }) {
  const items = [
    { label: '2xx', count: snap.status_2xx, color: '#3fb950' },
    { label: '4xx', count: snap.status_4xx, color: '#d29922' },
    { label: '5xx', count: snap.status_5xx, color: '#f85149' },
    { label: 'failed', count: snap.connections_failed, color: '#f85149' },
    { label: 'timeout', count: snap.connections_timed_out ?? 0, color: '#d29922' },
  ]
  const total = snap.responses_total + snap.connections_failed

  return (
    <div style={chartStyle}>
      <div className="card-title" style={{ marginBottom: 8 }}>Error Breakdown</div>
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
                <div style={{
                  height: '100%', width: `${pct}%`,
                  background: color, borderRadius: 2, transition: 'width 0.3s',
                }} />
              </div>
            </div>
          )
        })}
      </div>

      {/* Status code detail */}
      {(() => {
        const bd = snap.status_code_breakdown ?? {}
        const entries = Object.entries(bd).sort(([a], [b]) => Number(a) - Number(b))
        if (entries.length === 0) return null
        return (
          <div style={{ marginTop: 10, display: 'flex', flexWrap: 'wrap', gap: 6 }}>
            {entries.map(([code, count]) => {
              const c = Number(code)
              const color = c < 300 ? '#3fb950' : c < 400 ? '#58a6ff' : c < 500 ? '#d29922' : '#f85149'
              return (
                <div key={code} style={{ background: '#0d1117', borderRadius: 5, padding: '4px 8px', minWidth: 60 }}>
                  <div style={{ fontSize: 13, fontWeight: 700, color, fontFamily: 'monospace' }}>{code}</div>
                  <div style={{ fontSize: 11, color: '#8b949e' }}>{(count as number).toLocaleString()}</div>
                </div>
              )
            })}
          </div>
        )
      })()}
    </div>
  )
}

// ─── 헬퍼 ────────────────────────────────────────────────────────────────────

function buildChartData(history: MetricsSnapshot[]) {
  return history.map((s, i) => ({
    t: i,
    cps: +s.cps.toFixed(2),
    rps: +s.rps.toFixed(2),
    active: s.active_connections,
    bwTx: +(s.bytes_tx_per_sec / 1024).toFixed(2),
    bwRx: +(s.bytes_rx_per_sec / 1024).toFixed(2),
    latMean: +s.latency_mean_ms.toFixed(2),
    latP99: +s.latency_p99_ms.toFixed(2),
  }))
}

// ─── 메인 컴포넌트 ────────────────────────────────────────────────────────────

export default function MetricsPanel({ visibleCharts }: { visibleCharts: Set<ChartKey> }) {
  const { latestSnapshot: snap, snapshotHistory: history, activeProfile: profile } = useTestStore()

  if (!snap) {
    return (
      <div className="card" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', minHeight: 200 }}>
        <span style={{ color: '#8b949e', fontSize: 14 }}>Waiting for metrics…</span>
      </div>
    )
  }

  const chartData = buildChartData(history)
  const targetCps = profile?.test_type === 'cps' ? profile.default_load.cps_per_client : undefined
  const targetCc = profile?.default_load.cc_per_client
  const violations = snap.threshold_violations ?? []
  const isRampingUp = snap.is_ramping_up ?? false

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>

      {/* Ramp-up 배너 */}
      {isRampingUp && (
        <div style={{
          background: 'rgba(188,140,255,0.1)', border: '1px solid #bc8cff',
          borderRadius: 8, padding: '7px 14px', fontSize: 13, color: '#bc8cff',
          display: 'flex', alignItems: 'center', gap: 8,
        }}>
          <span style={{ fontWeight: 700 }}>↑ Ramping Up</span>
          <span style={{ color: '#8b949e' }}>— linearly scaling to target load</span>
        </div>
      )}

      {/* 임계값 위반 배너 */}
      {violations.length > 0 && (
        <div style={{
          background: 'rgba(248,81,73,0.1)', border: '1px solid #f85149',
          borderRadius: 8, padding: '7px 14px', fontSize: 13,
        }}>
          <div style={{ fontWeight: 700, color: '#f85149', marginBottom: 4 }}>⚠ Threshold Violation</div>
          {violations.map((v, i) => (
            <div key={i} style={{ color: '#e6ac3a', fontSize: 12 }}>• {v}</div>
          ))}
        </div>
      )}

      {/* 컴팩트 메트릭 테이블 */}
      <MetricsTable snap={snap} profile={profile} />

      {/* 선택된 차트만 렌더링 */}
      {chartData.length > 1 && visibleCharts.has('cpsRps') && (
        <CpsRpsChart data={chartData} targetCps={targetCps} />
      )}
      {chartData.length > 1 && visibleCharts.has('activeConn') && (
        <ActiveConnChart data={chartData} targetCc={targetCc} />
      )}
      {chartData.length > 1 && visibleCharts.has('bandwidth') && (
        <BandwidthChart data={chartData} />
      )}
      {chartData.length > 1 && visibleCharts.has('latency') && (
        <LatencyChart data={chartData} />
      )}
      {visibleCharts.has('histogram') && <LatencyHistogram snap={snap} />}
      {visibleCharts.has('errors') && <ErrorBreakdown snap={snap} />}

    </div>
  )
}
