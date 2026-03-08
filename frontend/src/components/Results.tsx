import { useState } from 'react'
import { Download, RefreshCw, Trash2 } from 'lucide-react'
import { useTestStore } from '../store/testStore'
import { TestResult } from '../api/client'
import { Card, CardContent } from './ui/card'
import { Button } from './ui/button'
import { Badge } from './ui/badge'
import { cn } from '@/lib/utils'

function formatTime(secs: number): string {
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${m}:${String(s).padStart(2, '0')}`
}

function formatDate(epochSecs: number): string {
  return new Date(epochSecs * 1000).toLocaleString()
}

function successRate(r: TestResult): number {
  const snap = r.final_snapshot
  return snap.responses_total > 0 ? (snap.status_2xx / snap.responses_total) * 100 : 0
}

function downloadJson(result: TestResult) {
  const blob = new Blob([JSON.stringify(result, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `result_${result.config.name.replace(/\s+/g, '_')}_${result.started_at_secs}.json`
  a.click()
  URL.revokeObjectURL(url)
}

function downloadCsv(results: TestResult[]) {
  const headers = [
    'id', 'name', 'test_type', 'protocol', 'started_at', 'elapsed_secs',
    'cps', 'rps', 'active_conn', 'success_rate_pct',
    'latency_p50_ms', 'latency_p99_ms', 'bytes_tx_total', 'bytes_rx_total',
    'connections_established', 'connections_failed',
  ]
  const rows = results.map((r) => {
    const s = r.final_snapshot
    const protocols = [...new Set(r.config.servers.map((s) => s.protocol))].join('/')
    return [
      r.id, r.config.name, r.config.test_type, protocols,
      formatDate(r.started_at_secs), r.elapsed_secs,
      s.cps.toFixed(2), s.rps.toFixed(2), s.active_connections,
      successRate(r).toFixed(1), s.latency_p50_ms.toFixed(2), s.latency_p99_ms.toFixed(2),
      s.bytes_tx_total, s.bytes_rx_total, s.connections_established, s.connections_failed,
    ].join(',')
  })
  const csv = [headers.join(','), ...rows].join('\n')
  const blob = new Blob([csv], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'net-meter-results.csv'
  a.click()
  URL.revokeObjectURL(url)
}

// ─── Detail panel ─────────────────────────────────────────────────────────────

function ResultDetail({ result }: { result: TestResult }) {
  const snap = result.final_snapshot
  const succ = successRate(result)

  return (
    <div className="px-4 py-4 flex flex-col gap-3">
      <div className="grid grid-cols-4 gap-2">
        {[
          { label: 'CPS (final)', value: snap.cps.toFixed(1) + '/s', cls: 'text-success' },
          { label: 'RPS (final)', value: snap.rps.toFixed(1) + '/s', cls: 'text-primary' },
          { label: 'Success Rate', value: succ.toFixed(1) + '%', cls: succ >= 99 ? 'text-success' : succ >= 90 ? 'text-warning' : 'text-destructive' },
          { label: 'Active Conn', value: String(snap.active_connections), cls: 'text-warning' },
          { label: 'Latency p50', value: snap.latency_p50_ms.toFixed(2) + 'ms', cls: 'text-success' },
          { label: 'Latency p99', value: snap.latency_p99_ms.toFixed(2) + 'ms', cls: 'text-destructive' },
          { label: 'TTFB p99', value: snap.ttfb_p99_ms.toFixed(2) + 'ms', cls: 'text-purple' },
          { label: 'Conn p99', value: snap.connect_p99_ms.toFixed(2) + 'ms', cls: 'text-warning' },
        ].map(({ label, value, cls }) => (
          <div key={label} className="bg-subtle rounded px-3 py-2.5">
            <div className="text-xs text-muted-foreground mb-1">{label}</div>
            <div className={cn('text-lg font-bold font-mono', cls)}>{value}</div>
          </div>
        ))}
      </div>

      <div className="grid grid-cols-3 gap-2">
        {[
          { label: 'Connections', value: snap.connections_established.toLocaleString() },
          { label: 'Total Requests', value: snap.requests_total.toLocaleString() },
          { label: 'Total Responses', value: snap.responses_total.toLocaleString() },
          { label: 'Failed', value: snap.connections_failed.toLocaleString() },
          { label: 'TX Total', value: (snap.bytes_tx_total / 1024 / 1024).toFixed(2) + ' MB' },
          { label: 'RX Total', value: (snap.bytes_rx_total / 1024 / 1024).toFixed(2) + ' MB' },
        ].map(({ label, value }) => (
          <div key={label} className="text-sm">
            <span className="text-muted-foreground">{label}: </span>
            <span className="text-foreground font-mono">{value}</span>
          </div>
        ))}
      </div>

      <div className="text-xs text-muted-foreground/50">
        Config: {result.config.test_type.toUpperCase()} · {result.config.associations.length} association(s) ·{' '}
        {[...new Set(result.config.servers.map((s) => s.protocol.toUpperCase()))].join('/')}
      </div>
    </div>
  )
}

// ─── Compare view ─────────────────────────────────────────────────────────────

interface MetricRow {
  label: string; va: number; vb: number
  fmt: (v: number) => string; lowerBetter?: boolean
}

function DeltaCell({ va, vb, fmt, lowerBetter }: MetricRow) {
  const d = vb - va
  const pct = va !== 0 ? (d / va) * 100 : 0
  const improved = lowerBetter ? d < 0 : d > 0
  const cls = Math.abs(d) < 1e-9 ? 'text-muted-foreground' : improved ? 'text-success' : 'text-destructive'
  const sign = d > 0 ? '+' : ''
  return (
    <div className={cn('text-center font-mono text-xs py-1.5', cls)}>
      {sign}{fmt(d)}<span className="text-[10px] ml-1">({sign}{pct.toFixed(1)}%)</span>
    </div>
  )
}

function ResultCompare({ a, b, onClose }: { a: TestResult; b: TestResult; onClose: () => void }) {
  const metrics: MetricRow[] = [
    { label: 'CPS', va: a.final_snapshot.cps, vb: b.final_snapshot.cps, fmt: v => v.toFixed(1) },
    { label: 'RPS', va: a.final_snapshot.rps, vb: b.final_snapshot.rps, fmt: v => v.toFixed(1) },
    { label: 'Success Rate (%)', va: successRate(a), vb: successRate(b), fmt: v => v.toFixed(1) },
    { label: 'Active Conn', va: a.final_snapshot.active_connections, vb: b.final_snapshot.active_connections, fmt: v => String(Math.round(v)) },
    { label: 'Latency p50 (ms)', va: a.final_snapshot.latency_p50_ms, vb: b.final_snapshot.latency_p50_ms, fmt: v => v.toFixed(2), lowerBetter: true },
    { label: 'Latency p99 (ms)', va: a.final_snapshot.latency_p99_ms, vb: b.final_snapshot.latency_p99_ms, fmt: v => v.toFixed(2), lowerBetter: true },
    { label: 'TTFB p99 (ms)', va: a.final_snapshot.ttfb_p99_ms, vb: b.final_snapshot.ttfb_p99_ms, fmt: v => v.toFixed(2), lowerBetter: true },
    { label: 'Connect p99 (ms)', va: a.final_snapshot.connect_p99_ms, vb: b.final_snapshot.connect_p99_ms, fmt: v => v.toFixed(2), lowerBetter: true },
    { label: 'TX Total (MB)', va: a.final_snapshot.bytes_tx_total / 1024 / 1024, vb: b.final_snapshot.bytes_tx_total / 1024 / 1024, fmt: v => v.toFixed(2) },
    { label: 'RX Total (MB)', va: a.final_snapshot.bytes_rx_total / 1024 / 1024, vb: b.final_snapshot.bytes_rx_total / 1024 / 1024, fmt: v => v.toFixed(2) },
    { label: 'Connections', va: a.final_snapshot.connections_established, vb: b.final_snapshot.connections_established, fmt: v => Math.round(v).toLocaleString() },
    { label: 'Failed', va: a.final_snapshot.connections_failed, vb: b.final_snapshot.connections_failed, fmt: v => String(Math.round(v)), lowerBetter: true },
  ]

  return (
    <Card>
      <CardContent className="p-0 overflow-hidden">
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <span className="font-semibold text-sm">Result Comparison</span>
          <Button variant="secondary" size="xs" onClick={onClose}>Close</Button>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full border-collapse">
            <thead>
              <tr className="bg-subtle text-[11px] uppercase tracking-wider text-muted-foreground">
                <th className="text-left px-3 py-2 font-semibold">Metric</th>
                <th className="text-left px-3 py-2 font-semibold text-primary">
                  {a.config.name}
                  <span className="text-[10px] text-muted-foreground/50 ml-2">{formatDate(a.started_at_secs)}</span>
                </th>
                <th className="text-center px-3 py-2 font-semibold">Delta (B - A)</th>
                <th className="text-left px-3 py-2 font-semibold text-warning">
                  {b.config.name}
                  <span className="text-[10px] text-muted-foreground/50 ml-2">{formatDate(b.started_at_secs)}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {metrics.map((m) => (
                <tr key={m.label} className="border-t border-border hover:bg-muted/40 transition-colors">
                  <td className="px-3 py-1.5 text-xs text-muted-foreground">{m.label}</td>
                  <td className="px-3 py-1.5 text-sm font-mono text-foreground">{m.fmt(m.va)}</td>
                  <td className="px-3 py-0"><DeltaCell {...m} /></td>
                  <td className="px-3 py-1.5 text-sm font-mono text-foreground">{m.fmt(m.vb)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </CardContent>
    </Card>
  )
}

// ─── Main ─────────────────────────────────────────────────────────────────────

export default function Results() {
  const { testResults, deleteResult, fetchResults } = useTestStore()
  const [expanded, setExpanded] = useState<string | null>(null)
  const [compareSet, setCompareSet] = useState<Set<string>>(new Set())
  const [showCompare, setShowCompare] = useState(false)

  const toggle = (id: string) => setExpanded((prev) => (prev === id ? null : id))

  const toggleCompare = (id: string) => {
    setCompareSet((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else if (next.size < 2) next.add(id)
      return next
    })
    setShowCompare(false)
  }

  const compareIds = [...compareSet]
  const compareResults =
    compareIds.length === 2
      ? ([testResults.find(r => r.id === compareIds[0]), testResults.find(r => r.id === compareIds[1])].filter(Boolean) as TestResult[])
      : null

  const testTypeBadge = (t: string): Parameters<typeof Badge>[0]['variant'] =>
    t === 'cps' ? 'cps' : t === 'bw' ? 'bw' : 'cc'

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-between items-center">
        <h2 className="text-base font-semibold">
          Test Results
          <span className="text-muted-foreground font-normal text-sm ml-2">({testResults.length} stored)</span>
        </h2>
        <div className="flex gap-2">
          {compareSet.size === 2 && (
            <Button size="sm" onClick={() => setShowCompare(v => !v)}>
              {showCompare ? 'Hide Compare' : 'Compare Selected'}
            </Button>
          )}
          {compareSet.size > 0 && (
            <Button variant="secondary" size="sm" onClick={() => { setCompareSet(new Set()); setShowCompare(false) }}>
              Clear ({compareSet.size}/2)
            </Button>
          )}
          <Button variant="secondary" size="sm" onClick={fetchResults}>
            <RefreshCw className="h-3.5 w-3.5" />
            Refresh
          </Button>
          {testResults.length > 0 && (
            <Button variant="secondary" size="sm" onClick={() => downloadCsv(testResults)}>
              <Download className="h-3.5 w-3.5" />
              Export CSV
            </Button>
          )}
        </div>
      </div>

      {showCompare && compareResults && compareResults.length === 2 && (
        <ResultCompare a={compareResults[0]} b={compareResults[1]} onClose={() => setShowCompare(false)} />
      )}

      {compareSet.size > 0 && !showCompare && (
        <div className="bg-muted border border-border rounded-lg px-4 py-2 text-xs text-muted-foreground">
          {compareSet.size === 1
            ? 'Select one more result to compare.'
            : 'Click "Compare Selected" to view side-by-side comparison.'}
        </div>
      )}

      {testResults.length === 0 ? (
        <Card>
          <CardContent className="text-muted-foreground text-sm text-center py-10">
            No test results yet. Run a test to see results here.
          </CardContent>
        </Card>
      ) : (
        <div className="flex flex-col gap-2">
          {testResults.map((r) => {
            const succ = successRate(r)
            const isOpen = expanded === r.id
            const isSelected = compareSet.has(r.id)

            return (
              <Card
                key={r.id}
                className={cn(
                  'overflow-hidden transition-colors',
                  isSelected && 'border-primary',
                )}
              >
                <div
                  onClick={() => toggle(r.id)}
                  className={cn(
                    'flex items-center gap-3 px-4 py-3 cursor-pointer transition-colors',
                    isOpen ? 'bg-muted/50' : 'hover:bg-muted/30',
                  )}
                >
                  <div onClick={(e) => { e.stopPropagation(); toggleCompare(r.id) }}>
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => {}}
                      disabled={!isSelected && compareSet.size >= 2}
                      className="w-3.5 h-3.5 cursor-pointer accent-[var(--primary)]"
                      title={isSelected ? 'Deselect' : compareSet.size >= 2 ? 'Max 2 selected' : 'Select for comparison'}
                    />
                  </div>

                  <span className="text-xs text-muted-foreground w-3">{isOpen ? '▼' : '▶'}</span>

                  <Badge variant={testTypeBadge(r.config.test_type)}>
                    {r.config.test_type.toUpperCase()}
                  </Badge>

                  <div className="flex-1 min-w-0">
                    <div className="font-semibold text-sm truncate">{r.config.name}</div>
                    <div className="text-xs text-muted-foreground">
                      {formatDate(r.started_at_secs)} · {formatTime(r.elapsed_secs)}
                    </div>
                  </div>

                  <div className="flex gap-5 text-xs shrink-0">
                    <div>
                      <span className="text-muted-foreground">CPS </span>
                      <span className="text-success font-bold font-mono">{r.final_snapshot.cps.toFixed(1)}</span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">p99 </span>
                      <span className="text-destructive font-bold font-mono">{r.final_snapshot.latency_p99_ms.toFixed(1)}ms</span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">succ </span>
                      <span className={cn('font-bold font-mono', succ >= 99 ? 'text-success' : succ >= 90 ? 'text-warning' : 'text-destructive')}>
                        {succ.toFixed(1)}%
                      </span>
                    </div>
                  </div>

                  <div className="flex gap-1.5 shrink-0" onClick={(e) => e.stopPropagation()}>
                    <Button variant="secondary" size="xs" onClick={() => downloadJson(r)}>
                      <Download className="h-3 w-3" />
                      JSON
                    </Button>
                    <Button variant="destructive" size="xs" onClick={() => deleteResult(r.id)}>
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </div>

                {isOpen && (
                  <div className="border-t border-border">
                    <ResultDetail result={r} />
                  </div>
                )}
              </Card>
            )
          })}
        </div>
      )}
    </div>
  )
}
