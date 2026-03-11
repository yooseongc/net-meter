import { useTestStore } from '../store/testStore'
import { MetricsSnapshot } from '../api/client'
import { Card, CardContent, CardTitle } from './ui/card'
import { cn } from '@/lib/utils'

function fmtKB(bps: number): string {
  if (bps >= 1e6) return `${(bps / 1e6).toFixed(1)} MB/s`
  if (bps >= 1e3) return `${(bps / 1e3).toFixed(1)} KB/s`
  return `${bps.toFixed(0)} B/s`
}

function fmtNum(n: number): string {
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`
  return n.toFixed(n < 10 ? 1 : 0)
}

function NodeBox({
  title, subtitle, stats, active,
}: {
  title: string
  subtitle: string
  stats?: { label: string; value: string; cls?: string }[]
  active: boolean
}) {
  return (
    <div
      className={cn(
        'rounded-lg p-3 min-w-[150px] transition-all duration-400',
        'border-2 bg-card',
        active
          ? 'border-success shadow-[0_0_10px_rgba(63,185,80,0.15)]'
          : 'border-border',
      )}
    >
      <div className="font-bold text-xs text-foreground mb-0.5">{title}</div>
      <div className="text-[10px] text-muted-foreground mb-1.5">{subtitle}</div>
      {stats && stats.length > 0 && (
        <div className="border-t border-border pt-1.5 flex flex-col gap-0.5">
          {stats.map(({ label, value, cls }) => (
            <div key={label} className="flex justify-between text-[10px]">
              <span className="text-muted-foreground">{label}</span>
              <span className={cn('font-semibold font-mono', cls ?? 'text-foreground')}>{value}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function Arrow({ animated, label }: { animated: boolean; label?: string }) {
  const color = animated ? 'var(--success)' : 'var(--border)'
  const dotColor = animated ? 'var(--primary)' : 'var(--muted)'
  return (
    <div className="flex flex-col items-center justify-center gap-1 min-w-[80px]">
      {label && <span className="text-[10px] text-muted-foreground/60 font-mono">{label}</span>}
      <div className="relative w-[70px] h-5">
        <div
          className="absolute top-1/2 left-0 right-0 h-0.5 -translate-y-1/2 transition-colors duration-400"
          style={{ background: color }}
        />
        <div
          className="absolute right-0 top-1/2 -translate-y-1/2 transition-colors duration-400"
          style={{ width: 0, height: 0, borderTop: '5px solid transparent', borderBottom: '5px solid transparent', borderLeft: `8px solid ${color}` }}
        />
        <div
          className="absolute top-[calc(50%+6px)] left-0 right-0 h-px transition-colors duration-400"
          style={{ background: `repeating-linear-gradient(90deg, ${dotColor} 0 4px, transparent 4px 8px)` }}
        />
      </div>
    </div>
  )
}

// ─── Compact Metrics Summary ──────────────────────────────────────────────────

function MetricRow({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div className="flex items-center justify-between py-1 border-b border-border last:border-0">
      <span className="text-[10px] text-muted-foreground font-medium">{label}</span>
      <span className={cn('text-xs font-bold font-mono tabular-nums', color ?? 'text-foreground')}>
        {value}
      </span>
    </div>
  )
}

function MetricsSummary({ snap }: { snap: MetricsSnapshot }) {
  const succ = snap.responses_total > 0 ? (snap.status_2xx / snap.responses_total) * 100 : 0
  const fail = snap.connections_failed + snap.status_4xx + snap.status_5xx

  return (
    <div className="flex flex-col justify-center h-full px-1">
      <MetricRow label="CPS"     value={`${fmtNum(snap.cps)}/s`}                          color="text-success" />
      <MetricRow label="RPS"     value={`${fmtNum(snap.rps)}/s`}                           color="text-primary" />
      <MetricRow label="Conn"    value={fmtNum(snap.active_connections)}                   color="text-warning" />
      <MetricRow label="Success" value={`${succ.toFixed(1)}%`}                             color={succ >= 99 ? 'text-success' : succ >= 90 ? 'text-warning' : 'text-destructive'} />
      <MetricRow label="Fail"    value={fmtNum(fail)}                                      color={fail > 0 ? 'text-destructive' : 'text-muted-foreground'} />
      <MetricRow label="TX"      value={fmtKB(snap.bytes_tx_per_sec)}                      color="text-warning" />
      <MetricRow label="RX"      value={fmtKB(snap.bytes_rx_per_sec)}                      color="text-purple" />
      <MetricRow label="Latency" value={`p50 ${snap.latency_p50_ms.toFixed(1)}ms / p99 ${snap.latency_p99_ms.toFixed(1)}ms`} color="text-purple" />
    </div>
  )
}

// ─── DUT 박스 (External Port 모드) ──────────────────────────────────────────────

function DutBox({ label }: { label: string }) {
  return (
    <div className="rounded-lg px-4 py-3 min-w-[110px] border-2 border-dashed border-muted-foreground/40 bg-muted/20 flex flex-col items-center justify-center gap-0.5">
      <div className="font-bold text-xs text-muted-foreground">{label}</div>
      <div className="text-[10px] text-muted-foreground/60">외부 장비</div>
    </div>
  )
}

// ─── Main ─────────────────────────────────────────────────────────────────────

export default function TopologyView({ compact = false }: { compact?: boolean }) {
  const { testState, latestSnapshot: snap, runtimeConfig } = useTestStore()
  const { mode: networkMode, upper_iface: upperIface, lower_iface: lowerIface } = runtimeConfig

  const isRunning = testState === 'running'
  const useNs = networkMode === 'namespace'
  const useExtPort = networkMode === 'external_port'

  const clientStats = snap ? [
    { label: 'CPS', value: snap.cps.toFixed(1) + '/s', cls: 'text-success' },
    { label: 'Active', value: String(snap.active_connections), cls: 'text-warning' },
    { label: 'TX', value: fmtKB(snap.bytes_tx_per_sec), cls: 'text-warning' },
    { label: 'Failed', value: String(snap.connections_failed), cls: snap.connections_failed > 0 ? 'text-destructive' : 'text-muted-foreground' },
  ] : undefined

  const serverStats = snap ? [
    { label: 'RPS', value: snap.rps.toFixed(1) + '/s', cls: 'text-primary' },
    { label: 'RX', value: fmtKB(snap.bytes_rx_per_sec), cls: 'text-purple' },
    { label: '2xx', value: snap.status_2xx.toLocaleString(), cls: 'text-success' },
    { label: 'Errors', value: (snap.status_4xx + snap.status_5xx).toLocaleString(), cls: snap.status_4xx + snap.status_5xx > 0 ? 'text-destructive' : 'text-muted-foreground' },
  ] : undefined

  // External Port 다이어그램: Generator ─upper─ DUT ─lower─ Responder
  const extPortDiagram = useExtPort ? (
    <div className={cn('flex items-center justify-center gap-0 flex-wrap', compact ? 'py-2' : 'py-8')}>
      <NodeBox
        title="Generator"
        subtitle={`Client · ${upperIface}`}
        stats={clientStats}
        active={isRunning}
      />
      <Arrow animated={isRunning} label={upperIface} />
      <DutBox label="DUT" />
      <Arrow animated={isRunning} label={lowerIface} />
      <NodeBox
        title="Responder"
        subtitle={`Server · ${lowerIface}`}
        stats={serverStats}
        active={isRunning}
      />
    </div>
  ) : null

  // NS 모드: prefix에서 NS 이름 추론
  const nsPrefix = upperIface.startsWith('veth-') ? 'nm' : upperIface.split('-')[0] ?? 'nm'
  const clientNs = `${nsPrefix}-client`
  const serverNs = `${nsPrefix}-server`

  const diagram = useExtPort ? extPortDiagram! : useNs ? (
    <div className={cn('flex items-center justify-center gap-0 flex-wrap', compact ? 'py-2' : 'py-8')}>
      <NodeBox
        title="Generator"
        subtitle={`[${clientNs}]\nclient IPs`}
        stats={clientStats}
        active={isRunning}
      />
      <Arrow animated={isRunning} label={upperIface} />
      <div className="rounded-lg px-3 py-2 border-2 border-dashed border-muted-foreground/40 bg-muted/10 flex flex-col items-center justify-center gap-0.5 min-w-[90px]">
        <div className="font-bold text-[10px] text-muted-foreground">Bridge / SW</div>
        <div className="text-[9px] text-muted-foreground/60">operator setup</div>
      </div>
      <Arrow animated={isRunning} label={lowerIface} />
      <NodeBox
        title="Responder"
        subtitle={`[${serverNs}]\nserver IPs`}
        stats={serverStats}
        active={isRunning}
      />
    </div>
  ) : (
    <div className={cn('flex items-center justify-center gap-0 flex-wrap', compact ? 'py-2' : 'py-8')}>
      <NodeBox title="Generator" subtitle="Client" stats={clientStats} active={isRunning} />
      <Arrow animated={isRunning} label="loopback" />
      <NodeBox title="Responder" subtitle="Server" stats={serverStats} active={isRunning} />
    </div>
  )

  if (compact) {
    return (
      <Card>
        <CardContent className="p-0 overflow-hidden">
          <div className="grid" style={{ gridTemplateColumns: '3fr 2fr' }}>
            <div className="p-3 border-r border-border">
              <div className="flex items-center justify-between mb-1">
                <CardTitle>Network Topology</CardTitle>
                <span className="text-xs text-muted-foreground">
                  {useExtPort
                    ? `Ext Port · ${upperIface} / ${lowerIface}`
                    : useNs ? `Namespace · ${clientNs} ↔ Bridge/SW ↔ ${serverNs}` : 'Loopback'}
                </span>
              </div>
              {diagram}
            </div>
            <div className="p-3">
              <CardTitle className="mb-2">Live Summary</CardTitle>
              {snap
                ? <MetricsSummary snap={snap} />
                : <div className="flex items-center justify-center h-full text-xs text-muted-foreground/50 italic">Waiting…</div>
              }
            </div>
          </div>
        </CardContent>
      </Card>
    )
  }

  return (
    <div className="flex flex-col gap-5">
      <Card>
        <CardContent className="text-sm text-muted-foreground">
          <span className="text-foreground font-semibold">Mode: </span>
          {useExtPort
            ? `External Port — Generator(${upperIface}) ↔ DUT ↔ Responder(${lowerIface})`
            : useNs
              ? `Namespace isolation — [${clientNs}] ↔ Bridge/SW (operator) ↔ [${serverNs}]`
              : 'Local mode — generator and responder on localhost'}
        </CardContent>
      </Card>

      {diagram}

      {snap && (
        <Card>
          <CardContent>
            <CardTitle className="mb-3">Live Metrics Summary</CardTitle>
            <div className="grid grid-cols-4 gap-4">
              {[
                { label: 'Total Connections', value: snap.connections_established.toLocaleString() },
                { label: 'Total Requests', value: snap.requests_total.toLocaleString() },
                { label: 'Total Responses', value: snap.responses_total.toLocaleString() },
                { label: 'Total TX', value: fmtKB(snap.bytes_tx_total) },
                { label: 'TTFB mean', value: snap.ttfb_mean_ms.toFixed(2) + 'ms' },
                { label: 'TTFB p99', value: snap.ttfb_p99_ms.toFixed(2) + 'ms' },
                { label: 'Connect mean', value: snap.connect_mean_ms.toFixed(2) + 'ms' },
                { label: 'Connect p99', value: snap.connect_p99_ms.toFixed(2) + 'ms' },
              ].map(({ label, value }) => (
                <div key={label}>
                  <div className="text-xs text-muted-foreground mb-0.5">{label}</div>
                  <div className="text-base font-bold font-mono text-foreground">{value}</div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
