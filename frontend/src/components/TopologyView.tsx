import { useTestStore } from '../store/testStore'

function fmtKB(bps: number): string {
  if (bps >= 1e6) return `${(bps / 1e6).toFixed(1)} MB/s`
  if (bps >= 1e3) return `${(bps / 1e3).toFixed(1)} KB/s`
  return `${bps.toFixed(0)} B/s`
}

function NodeBox({
  title,
  subtitle,
  ip,
  iface,
  stats,
  active,
}: {
  title: string
  subtitle: string
  ip?: string
  iface?: string
  stats?: { label: string; value: string; color?: string }[]
  active: boolean
}) {
  return (
    <div
      style={{
        background: '#161b22',
        border: `2px solid ${active ? '#3fb950' : '#30363d'}`,
        borderRadius: 10,
        padding: '14px 18px',
        minWidth: 180,
        transition: 'border-color 0.4s',
        boxShadow: active ? '0 0 12px rgba(63,185,80,0.2)' : 'none',
      }}
    >
      <div style={{ fontWeight: 700, fontSize: 14, color: '#e6edf3', marginBottom: 4 }}>{title}</div>
      <div style={{ fontSize: 11, color: '#8b949e', marginBottom: 8 }}>{subtitle}</div>
      {ip && (
        <div style={{ fontSize: 11, color: '#58a6ff', fontFamily: 'monospace', marginBottom: 2 }}>
          {ip}
        </div>
      )}
      {iface && (
        <div style={{ fontSize: 11, color: '#484f58', fontFamily: 'monospace', marginBottom: 8 }}>
          {iface}
        </div>
      )}
      {stats && stats.length > 0 && (
        <div
          style={{
            borderTop: '1px solid #21262d',
            paddingTop: 8,
            display: 'flex',
            flexDirection: 'column',
            gap: 4,
          }}
        >
          {stats.map(({ label, value, color }) => (
            <div key={label} style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12 }}>
              <span style={{ color: '#8b949e' }}>{label}</span>
              <span style={{ color: color ?? '#e6edf3', fontWeight: 600, fontFamily: 'monospace' }}>
                {value}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function Arrow({ animated, label }: { animated: boolean; label?: string }) {
  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 4,
        minWidth: 80,
      }}
    >
      {label && (
        <span style={{ fontSize: 10, color: '#484f58', fontFamily: 'monospace' }}>{label}</span>
      )}
      <div style={{ position: 'relative', width: 70, height: 20 }}>
        {/* 화살표 줄기 */}
        <div
          style={{
            position: 'absolute',
            top: '50%',
            left: 0,
            right: 0,
            height: 2,
            background: animated ? '#3fb950' : '#30363d',
            transform: 'translateY(-50%)',
            transition: 'background 0.4s',
          }}
        />
        {/* 화살표 머리 */}
        <div
          style={{
            position: 'absolute',
            right: 0,
            top: '50%',
            transform: 'translateY(-50%)',
            width: 0,
            height: 0,
            borderTop: '5px solid transparent',
            borderBottom: '5px solid transparent',
            borderLeft: `8px solid ${animated ? '#3fb950' : '#30363d'}`,
            transition: 'border-color 0.4s',
          }}
        />
        {/* 역방향 점선 */}
        <div
          style={{
            position: 'absolute',
            top: 'calc(50% + 6px)',
            left: 0,
            right: 0,
            height: 1,
            background: `repeating-linear-gradient(90deg, ${animated ? '#58a6ff' : '#21262d'} 0 4px, transparent 4px 8px)`,
            transition: 'background 0.4s',
          }}
        />
      </div>
    </div>
  )
}

export default function TopologyView() {
  const { testState, activeProfile, latestSnapshot: snap } = useTestStore()

  const isRunning = testState === 'running'
  const useNs = activeProfile?.use_namespace ?? false
  const prefix = activeProfile?.netns_prefix ?? 'nm'

  const clientStats = snap
    ? [
        { label: 'CPS', value: snap.cps.toFixed(1) + '/s', color: '#3fb950' },
        { label: 'Active', value: String(snap.active_connections), color: '#d29922' },
        { label: 'TX', value: fmtKB(snap.bytes_tx_per_sec), color: '#d29922' },
        { label: 'Failed', value: String(snap.connections_failed), color: snap.connections_failed > 0 ? '#f85149' : '#8b949e' },
      ]
    : undefined

  const serverStats = snap
    ? [
        { label: 'RPS', value: snap.rps.toFixed(1) + '/s', color: '#58a6ff' },
        { label: 'RX', value: fmtKB(snap.bytes_rx_per_sec), color: '#bc8cff' },
        { label: '2xx', value: snap.status_2xx.toLocaleString(), color: '#3fb950' },
        { label: '4xx+5xx', value: (snap.status_4xx + snap.status_5xx).toLocaleString(), color: snap.status_4xx + snap.status_5xx > 0 ? '#f85149' : '#8b949e' },
      ]
    : undefined

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      {/* 설명 배너 */}
      <div className="card" style={{ fontSize: 13, color: '#8b949e' }}>
        <span style={{ color: '#e6edf3', fontWeight: 600 }}>Mode: </span>
        {useNs
          ? `Namespace isolation — client NS (${prefix}-client) ↔ server NS (${prefix}-server)`
          : 'Local mode — generator and responder on localhost'}
      </div>

      {/* 토폴로지 다이어그램 */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 0,
          flexWrap: 'wrap',
          padding: '32px 0',
        }}
      >
        {useNs ? (
          <>
            {/* Client NS */}
            <NodeBox
              title="Client NS"
              subtitle={`${prefix}-client`}
              ip="10.10.0.2/30"
              iface="veth-c1"
              stats={clientStats}
              active={isRunning}
            />
            <Arrow animated={isRunning} label="veth pair" />

            {/* Host */}
            <NodeBox
              title="Host"
              subtitle="IP Forwarding"
              ip="10.10.0.1/30 · 10.20.0.1/30"
              iface="veth-c0 · veth-s0"
              stats={
                snap
                  ? [
                      { label: 'Latency p50', value: snap.latency_p50_ms.toFixed(2) + 'ms', color: '#3fb950' },
                      { label: 'Latency p99', value: snap.latency_p99_ms.toFixed(2) + 'ms', color: '#f85149' },
                    ]
                  : undefined
              }
              active={isRunning}
            />
            <Arrow animated={isRunning} label="veth pair" />

            {/* Server NS */}
            <NodeBox
              title="Server NS"
              subtitle={`${prefix}-server`}
              ip="10.20.0.2/30"
              iface="veth-s1"
              stats={serverStats}
              active={isRunning}
            />
          </>
        ) : (
          <>
            {/* Local mode */}
            <NodeBox
              title="Generator"
              subtitle="HTTP/1.1 Client"
              ip={`→ ${activeProfile?.target_host ?? '127.0.0.1'}:${activeProfile?.target_port ?? 8080}`}
              stats={clientStats}
              active={isRunning}
            />
            <Arrow animated={isRunning} label="loopback" />
            <NodeBox
              title="Responder"
              subtitle="HTTP Server"
              ip={`0.0.0.0:${activeProfile?.target_port ?? 8080}`}
              stats={serverStats}
              active={isRunning}
            />
          </>
        )}
      </div>

      {/* 실시간 요약 테이블 */}
      {snap && (
        <div className="card">
          <div className="card-title">Live Metrics Summary</div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16 }}>
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
                <div style={{ fontSize: 11, color: '#8b949e', marginBottom: 2 }}>{label}</div>
                <div style={{ fontSize: 16, fontWeight: 700, fontFamily: 'monospace' }}>{value}</div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
