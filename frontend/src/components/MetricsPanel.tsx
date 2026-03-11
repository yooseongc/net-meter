import { useState } from 'react'
import {
  LineChart, Line, BarChart, Bar, AreaChart, Area,
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
  ReferenceLine, Legend,
} from 'recharts'
import { useTestStore } from '../store/testStore'
import { MetricsSnapshot } from '../api/client'
import { useChartColors } from '../lib/theme'
import { Card, CardContent, CardTitle } from './ui/card'
import { cn } from '@/lib/utils'
import { ChartKey } from './TestRunPanel'

const RANGE_OPTIONS = [
  { label: '30s', value: 30 },
  { label: '1m', value: 60 },
  { label: '2m', value: 120 },
  { label: '5m', value: 300 },
] as const

// ─── Charts ───────────────────────────────────────────────────────────────────

function ChartCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Card>
      <CardContent>
        <CardTitle className="mb-2">{title}</CardTitle>
        {children}
      </CardContent>
    </Card>
  )
}

type ChartData = ReturnType<typeof buildChartData>

function xAxisProps(range: number, c: ReturnType<typeof useChartColors>) {
  return {
    dataKey: 't' as const,
    type: 'number' as const,
    domain: [0, range - 1] as [number, number],
    tickCount: 7,
    tickFormatter: (v: number) => `${v}s`,
    tick: { fontSize: 11, fill: c.axis },
    allowDataOverflow: true,
  }
}

function CpsRpsChart({ data, targetCps, range }: { data: ChartData; targetCps?: number; range: number }) {
  const c = useChartColors()
  return (
    <ChartCard title="Connections & Requests / sec">
      <ResponsiveContainer width="100%" height={160}>
        <LineChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={c.grid} />
          <XAxis {...xAxisProps(range, c)} />
          <YAxis tick={{ fontSize: 11, fill: c.axis }} />
          <Tooltip contentStyle={{ background: c.tooltipBg, border: `1px solid ${c.tooltipBorder}`, fontSize: 12, color: c.tooltipColor }} />
          <Legend wrapperStyle={{ fontSize: 12 }} />
          {targetCps != null && (
            <ReferenceLine y={targetCps} stroke="var(--success)" strokeDasharray="6 3"
              label={{ value: `target ${targetCps}`, fill: 'var(--success)', fontSize: 10 }} />
          )}
          <Line type="linear" dataKey="cps" stroke="var(--success)" dot={false} name="CPS" strokeWidth={2} connectNulls={false} />
          <Line type="linear" dataKey="rps" stroke="var(--primary)" dot={false} name="RPS" strokeWidth={1.5} connectNulls={false} />
        </LineChart>
      </ResponsiveContainer>
    </ChartCard>
  )
}

function ActiveConnChart({ data, targetCc, range }: { data: ChartData; targetCc?: number; range: number }) {
  const c = useChartColors()
  return (
    <ChartCard title="Active Connections">
      <ResponsiveContainer width="100%" height={130}>
        <AreaChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <defs>
            <linearGradient id="activeGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="var(--warning)" stopOpacity={0.3} />
              <stop offset="95%" stopColor="var(--warning)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke={c.grid} />
          <XAxis {...xAxisProps(range, c)} />
          <YAxis tick={{ fontSize: 11, fill: c.axis }} />
          <Tooltip contentStyle={{ background: c.tooltipBg, border: `1px solid ${c.tooltipBorder}`, fontSize: 12, color: c.tooltipColor }} />
          {targetCc != null && (
            <ReferenceLine y={targetCc} stroke="var(--warning)" strokeDasharray="6 3"
              label={{ value: `target ${targetCc}`, fill: 'var(--warning)', fontSize: 10 }} />
          )}
          <Area type="linear" dataKey="active" stroke="var(--warning)" fill="url(#activeGrad)"
            dot={false} name="Active Conn" strokeWidth={2} connectNulls={false} />
        </AreaChart>
      </ResponsiveContainer>
    </ChartCard>
  )
}

function BandwidthChart({ data, range }: { data: ChartData; range: number }) {
  const c = useChartColors()
  return (
    <ChartCard title="Bandwidth (KB/s)">
      <ResponsiveContainer width="100%" height={130}>
        <AreaChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <defs>
            <linearGradient id="txGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="var(--warning)" stopOpacity={0.4} />
              <stop offset="95%" stopColor="var(--warning)" stopOpacity={0} />
            </linearGradient>
            <linearGradient id="rxGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="var(--purple)" stopOpacity={0.4} />
              <stop offset="95%" stopColor="var(--purple)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke={c.grid} />
          <XAxis {...xAxisProps(range, c)} />
          <YAxis tick={{ fontSize: 11, fill: c.axis }} />
          <Tooltip contentStyle={{ background: c.tooltipBg, border: `1px solid ${c.tooltipBorder}`, fontSize: 12, color: c.tooltipColor }} />
          <Legend wrapperStyle={{ fontSize: 12 }} />
          <Area type="linear" dataKey="bwTx" stroke="var(--warning)" fill="url(#txGrad)" dot={false} name="TX KB/s" strokeWidth={1.5} connectNulls={false} />
          <Area type="linear" dataKey="bwRx" stroke="var(--purple)" fill="url(#rxGrad)" dot={false} name="RX KB/s" strokeWidth={1.5} connectNulls={false} />
        </AreaChart>
      </ResponsiveContainer>
    </ChartCard>
  )
}

function LatencyChart({ data, range }: { data: ChartData; range: number }) {
  const c = useChartColors()
  return (
    <ChartCard title="Latency (ms)">
      <ResponsiveContainer width="100%" height={130}>
        <LineChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={c.grid} />
          <XAxis {...xAxisProps(range, c)} />
          <YAxis tick={{ fontSize: 11, fill: c.axis }} />
          <Tooltip contentStyle={{ background: c.tooltipBg, border: `1px solid ${c.tooltipBorder}`, fontSize: 12, color: c.tooltipColor }} />
          <Legend wrapperStyle={{ fontSize: 12 }} />
          <Line type="linear" dataKey="latMean" stroke="var(--primary)" dot={false} name="mean" strokeWidth={1.5} connectNulls={false} />
          <Line type="linear" dataKey="latP99" stroke="var(--destructive)" dot={false} name="p99" strokeWidth={1.5} connectNulls={false} />
        </LineChart>
      </ResponsiveContainer>
    </ChartCard>
  )
}

const DEFAULT_LE = [0.5, 1, 2, 5, 10, 25, 50, 100, 250, 500, null] as const

function formatLatencyBucketLabel(leMs: number | null | undefined) {
  if (leMs == null) return 'Inf'
  return leMs >= 1000 ? `${leMs / 1000}s` : `${leMs}ms`
}

function LatencyHistogram({ snap }: { snap: MetricsSnapshot }) {
  const c = useChartColors()
  const buckets = (snap.latency_histogram && snap.latency_histogram.length > 0)
    ? snap.latency_histogram
    : DEFAULT_LE.map(le => ({ le_ms: le, count: 0 }))
  const barData = buckets
    .slice(0, -1)
    .map((b, i) => ({
      le: formatLatencyBucketLabel(b.le_ms),
      count: i === 0 ? b.count : b.count - buckets[i - 1].count,
    }))

  return (
    <ChartCard title="Latency Distribution">
      <div className="flex gap-4 mb-2">
        {[
          { label: 'p50', value: snap.latency_p50_ms, cls: 'text-success' },
          { label: 'p95', value: snap.latency_p95_ms, cls: 'text-warning' },
          { label: 'p99', value: snap.latency_p99_ms, cls: 'text-destructive' },
          { label: 'max', value: snap.latency_max_ms, cls: 'text-muted-foreground' },
        ].map(({ label, value, cls }) => (
          <div key={label} className="text-xs">
            <span className="text-muted-foreground">{label}: </span>
            <span className={cn('font-bold font-mono', cls)}>{value.toFixed(2)}ms</span>
          </div>
        ))}
      </div>
      <ResponsiveContainer width="100%" height={120}>
        <BarChart data={barData} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke={c.grid} />
          <XAxis dataKey="le" tick={{ fontSize: 11, fill: c.axis }} />
          <YAxis tick={{ fontSize: 11, fill: c.axis }} />
          <Tooltip contentStyle={{ background: c.tooltipBg, border: `1px solid ${c.tooltipBorder}`, fontSize: 12, color: c.tooltipColor }} />
          <Bar dataKey="count" fill="var(--primary)" name="Requests" radius={[2, 2, 0, 0]} />
        </BarChart>
      </ResponsiveContainer>
    </ChartCard>
  )
}

function ErrorBreakdown({ snap }: { snap: MetricsSnapshot }) {
  const items = [
    { label: '2xx', count: snap.status_2xx, cls: 'text-success', barCls: 'bg-success' },
    { label: '4xx', count: snap.status_4xx, cls: 'text-warning', barCls: 'bg-warning' },
    { label: '5xx', count: snap.status_5xx, cls: 'text-destructive', barCls: 'bg-destructive' },
    { label: 'failed', count: snap.connections_failed, cls: 'text-destructive', barCls: 'bg-destructive' },
    { label: 'timeout', count: snap.connections_timed_out ?? 0, cls: 'text-warning', barCls: 'bg-warning' },
  ]
  const total = snap.responses_total + snap.connections_failed

  return (
    <ChartCard title="Error Breakdown">
      <div className="flex flex-col gap-2">
        {items.map(({ label, count, cls, barCls }) => {
          const pct = total > 0 ? ((count / total) * 100).toFixed(1) : '0.0'
          return (
            <div key={label}>
              <div className="flex justify-between text-xs mb-0.5">
                <span className={cls}>{label}</span>
                <span className="text-muted-foreground font-mono">
                  {count.toLocaleString()}{' '}
                  <span className="text-muted-foreground/50">({pct}%)</span>
                </span>
              </div>
              <div className="h-[3px] bg-muted rounded-full">
                <div
                  className={cn('h-full rounded-full transition-[width] duration-300', barCls)}
                  style={{ width: `${pct}%` }}
                />
              </div>
            </div>
          )
        })}
      </div>

      {(() => {
        const bd = snap.status_code_breakdown ?? {}
        const entries = Object.entries(bd).sort(([a], [b]) => Number(a) - Number(b))
        if (entries.length === 0) return null
        return (
          <div className="mt-3 flex flex-wrap gap-1.5">
            {entries.map(([code, count]) => {
              const c = Number(code)
              const cls = c < 300 ? 'text-success' : c < 400 ? 'text-primary' : c < 500 ? 'text-warning' : 'text-destructive'
              return (
                <div key={code} className="bg-subtle rounded px-2 py-1 min-w-[52px]">
                  <div className={cn('text-sm font-bold font-mono', cls)}>{code}</div>
                  <div className="text-xs text-muted-foreground">{(count as number).toLocaleString()}</div>
                </div>
              )
            })}
          </div>
        )
      })()}
    </ChartCard>
  )
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function buildChartData(history: MetricsSnapshot[], range: number) {
  const slice = history.slice(-range)
  const pad = range - slice.length
  return Array.from({ length: range }, (_, i) => {
    const idx = i - pad
    if (idx < 0) return { t: i }
    const s = slice[idx]
    return {
      t: i,
      cps: +s.cps.toFixed(2),
      rps: +s.rps.toFixed(2),
      active: s.active_connections,
      bwTx: +(s.bytes_tx_per_sec / 1024).toFixed(2),
      bwRx: +(s.bytes_rx_per_sec / 1024).toFixed(2),
      latMean: +s.latency_mean_ms.toFixed(2),
      latP99: +s.latency_p99_ms.toFixed(2),
    }
  })
}

// ─── Main ─────────────────────────────────────────────────────────────────────

export default function MetricsPanel({ visibleCharts }: { visibleCharts: Set<ChartKey> }) {
  const { latestSnapshot: snap, snapshotHistory: history, activeProfile: profile } = useTestStore()
  const [chartRange, setChartRange] = useState<number>(60)

  if (!snap) {
    return (
      <Card>
        <CardContent className="flex items-center justify-center min-h-[200px]">
          <span className="text-muted-foreground text-sm">Waiting for metrics…</span>
        </CardContent>
      </Card>
    )
  }

  const chartData = buildChartData(history, chartRange)
  const targetCps = undefined // CPS is organic output in tight-loop mode
  const targetCc = (profile?.test_type === 'cc' || profile?.test_type === 'bw')
    ? (profile.default_load.num_connections ?? undefined)
    : undefined
  const violations = snap.threshold_violations ?? []
  const isRampingUp = snap.is_ramping_up ?? false

  const hasTimeCharts = visibleCharts.has('cpsRps') || visibleCharts.has('activeConn')
    || visibleCharts.has('bandwidth') || visibleCharts.has('latency')

  return (
    <div className="flex flex-col gap-2.5">
      {isRampingUp && (
        <div className="bg-purple/10 border border-purple rounded-lg px-3.5 py-2 text-sm text-purple flex items-center gap-2">
          <span className="font-bold">↑ Ramping Up</span>
          <span className="text-muted-foreground">— linearly scaling to target load</span>
        </div>
      )}

      {violations.length > 0 && (
        <div className="bg-destructive/10 border border-destructive rounded-lg px-3.5 py-2 text-sm">
          <div className="font-bold text-destructive mb-1">⚠ Threshold Violation</div>
          {violations.map((v, i) => (
            <div key={i} className="text-warning text-xs">• {v}</div>
          ))}
        </div>
      )}

      {/* Range selector — 시계열 차트가 하나라도 보일 때만 */}
      {hasTimeCharts && (
        <div className="flex items-center gap-1.5">
          <span className="text-[11px] text-muted-foreground font-medium">Range</span>
          {RANGE_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              onClick={() => setChartRange(opt.value)}
              className={cn(
                'px-2 py-0.5 rounded text-[11px] font-semibold border transition-colors',
                chartRange === opt.value
                  ? 'bg-primary text-primary-foreground border-primary'
                  : 'bg-transparent text-muted-foreground border-border hover:text-foreground hover:border-foreground/30',
              )}
            >
              {opt.label}
            </button>
          ))}
        </div>
      )}

      {/* 차트: 1280px 이상에서 2컬럼 */}
      <div className="grid grid-cols-1 xl:grid-cols-2 gap-3">
        {visibleCharts.has('cpsRps') && (
          <CpsRpsChart data={chartData} targetCps={targetCps} range={chartRange} />
        )}
        {visibleCharts.has('activeConn') && (
          <ActiveConnChart data={chartData} targetCc={targetCc} range={chartRange} />
        )}
        {visibleCharts.has('bandwidth') && (
          <BandwidthChart data={chartData} range={chartRange} />
        )}
        {visibleCharts.has('latency') && (
          <LatencyChart data={chartData} range={chartRange} />
        )}
        {visibleCharts.has('histogram') && <LatencyHistogram snap={snap} />}
        {visibleCharts.has('errors') && <ErrorBreakdown snap={snap} />}
      </div>
    </div>
  )
}
