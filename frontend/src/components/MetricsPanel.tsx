import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts'
import { useTestStore } from '../store/testStore'

export default function MetricsPanel() {
  const { latestSnapshot: snap, snapshotHistory: history } = useTestStore()

  if (!snap) {
    return (
      <div className="card" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', minHeight: 300 }}>
        <span style={{ color: '#8b949e', fontSize: 14 }}>Waiting for metrics…</span>
      </div>
    )
  }

  const chartData = history.map((s, i) => ({
    t: i,
    cps: s.cps.toFixed(1),
    rps: s.rps.toFixed(1),
    latency: s.latency_mean_ms.toFixed(2),
    active: s.active_connections,
    bwTx: (s.bytes_tx_per_sec / 1024).toFixed(1),
    bwRx: (s.bytes_rx_per_sec / 1024).toFixed(1),
  }))

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* 핵심 지표 카드 */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12 }}>
        <StatCard label="CPS" value={snap.cps.toFixed(1)} unit="/s" color="#3fb950" />
        <StatCard label="RPS" value={snap.rps.toFixed(1)} unit="/s" color="#58a6ff" />
        <StatCard label="Active Conn" value={String(snap.active_connections)} unit="" color="#d29922" />
        <StatCard label="Latency (mean)" value={snap.latency_mean_ms.toFixed(2)} unit="ms" color="#bc8cff" />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12 }}>
        <StatCard label="Attempted" value={String(snap.connections_attempted)} unit="" color="#8b949e" />
        <StatCard label="Established" value={String(snap.connections_established)} unit="" color="#3fb950" />
        <StatCard label="Failed" value={String(snap.connections_failed)} unit="" color="#f85149" />
        <StatCard
          label="Success Rate"
          value={
            snap.responses_total > 0
              ? ((snap.status_2xx / snap.responses_total) * 100).toFixed(1)
              : '0.0'
          }
          unit="%"
          color={snap.responses_total > 0 && snap.status_2xx / snap.responses_total > 0.99 ? '#3fb950' : '#d29922'}
        />
      </div>

      {/* CPS / RPS 시계열 차트 */}
      {chartData.length > 1 && (
        <div className="card">
          <div className="card-title">Connections & Requests per Second</div>
          <ResponsiveContainer width="100%" height={200}>
            <LineChart data={chartData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#21262d" />
              <XAxis dataKey="t" tick={{ fontSize: 11, fill: '#8b949e' }} />
              <YAxis tick={{ fontSize: 11, fill: '#8b949e' }} />
              <Tooltip
                contentStyle={{ background: '#161b22', border: '1px solid #30363d', fontSize: 12 }}
              />
              <Legend wrapperStyle={{ fontSize: 12 }} />
              <Line type="monotone" dataKey="cps" stroke="#3fb950" dot={false} name="CPS" />
              <Line type="monotone" dataKey="rps" stroke="#58a6ff" dot={false} name="RPS" />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}

      {/* Bandwidth 시계열 차트 */}
      {chartData.length > 1 && (
        <div className="card">
          <div className="card-title">Bandwidth (KB/s)</div>
          <ResponsiveContainer width="100%" height={150}>
            <LineChart data={chartData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#21262d" />
              <XAxis dataKey="t" tick={{ fontSize: 11, fill: '#8b949e' }} />
              <YAxis tick={{ fontSize: 11, fill: '#8b949e' }} />
              <Tooltip
                contentStyle={{ background: '#161b22', border: '1px solid #30363d', fontSize: 12 }}
              />
              <Legend wrapperStyle={{ fontSize: 12 }} />
              <Line type="monotone" dataKey="bwTx" stroke="#d29922" dot={false} name="TX KB/s" />
              <Line type="monotone" dataKey="bwRx" stroke="#bc8cff" dot={false} name="RX KB/s" />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}

      {/* 상태 코드 분포 */}
      <div className="card">
        <div className="card-title">Response Status Distribution</div>
        <div style={{ display: 'flex', gap: 16 }}>
          <StatusBar label="2xx" count={snap.status_2xx} total={snap.responses_total} color="#3fb950" />
          <StatusBar label="4xx" count={snap.status_4xx} total={snap.responses_total} color="#d29922" />
          <StatusBar label="5xx" count={snap.status_5xx} total={snap.responses_total} color="#f85149" />
          <StatusBar label="other" count={snap.status_other} total={snap.responses_total} color="#8b949e" />
        </div>
      </div>
    </div>
  )
}

function StatCard({
  label,
  value,
  unit,
  color,
}: {
  label: string
  value: string
  unit: string
  color: string
}) {
  return (
    <div className="card">
      <div className="card-title">{label}</div>
      <div style={{ fontSize: 28, fontWeight: 700, color }}>
        {value}
        <span style={{ fontSize: 14, color: '#8b949e', marginLeft: 4 }}>{unit}</span>
      </div>
    </div>
  )
}

function StatusBar({
  label,
  count,
  total,
  color,
}: {
  label: string
  count: number
  total: number
  color: string
}) {
  const pct = total > 0 ? ((count / total) * 100).toFixed(1) : '0.0'
  return (
    <div style={{ flex: 1 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12, marginBottom: 4 }}>
        <span style={{ color }}>{label}</span>
        <span style={{ color: '#8b949e' }}>{count.toLocaleString()} ({pct}%)</span>
      </div>
      <div style={{ height: 4, background: '#21262d', borderRadius: 2 }}>
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
}
