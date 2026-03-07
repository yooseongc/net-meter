import { useState } from 'react'
import { useTestStore } from '../store/testStore'
import { TestResult } from '../api/client'

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
    const protocols = [...new Set(r.config.associations.map((a) => a.protocol))].join('/')
    return [
      r.id,
      r.config.name,
      r.config.test_type,
      protocols,
      formatDate(r.started_at_secs),
      r.elapsed_secs,
      s.cps.toFixed(2),
      s.rps.toFixed(2),
      s.active_connections,
      successRate(r).toFixed(1),
      s.latency_p50_ms.toFixed(2),
      s.latency_p99_ms.toFixed(2),
      s.bytes_tx_total,
      s.bytes_rx_total,
      s.connections_established,
      s.connections_failed,
    ].join(',')
  })
  const csv = [headers.join(','), ...rows].join('\n')
  const blob = new Blob([csv], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `net-meter-results.csv`
  a.click()
  URL.revokeObjectURL(url)
}

function ResultDetail({ result }: { result: TestResult }) {
  const snap = result.final_snapshot
  const succ = successRate(result)

  return (
    <div style={{ padding: '16px 0', display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 10 }}>
        {[
          { label: 'CPS (final)', value: snap.cps.toFixed(1) + '/s', color: '#3fb950' },
          { label: 'RPS (final)', value: snap.rps.toFixed(1) + '/s', color: '#58a6ff' },
          { label: 'Success Rate', value: succ.toFixed(1) + '%', color: succ >= 99 ? '#3fb950' : succ >= 90 ? '#d29922' : '#f85149' },
          { label: 'Active Conn', value: String(snap.active_connections), color: '#d29922' },
          { label: 'Latency p50', value: snap.latency_p50_ms.toFixed(2) + 'ms', color: '#3fb950' },
          { label: 'Latency p99', value: snap.latency_p99_ms.toFixed(2) + 'ms', color: '#f85149' },
          { label: 'TTFB p99', value: snap.ttfb_p99_ms.toFixed(2) + 'ms', color: '#bc8cff' },
          { label: 'Conn p99', value: snap.connect_p99_ms.toFixed(2) + 'ms', color: '#d29922' },
        ].map(({ label, value, color }) => (
          <div key={label} style={{ background: '#0d1117', borderRadius: 6, padding: '10px 12px' }}>
            <div style={{ fontSize: 11, color: '#8b949e', marginBottom: 4 }}>{label}</div>
            <div style={{ fontSize: 18, fontWeight: 700, color, fontFamily: 'monospace' }}>{value}</div>
          </div>
        ))}
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 10 }}>
        {[
          { label: 'Connections', value: snap.connections_established.toLocaleString() },
          { label: 'Total Requests', value: snap.requests_total.toLocaleString() },
          { label: 'Total Responses', value: snap.responses_total.toLocaleString() },
          { label: 'Failed', value: snap.connections_failed.toLocaleString() },
          { label: 'TX Total', value: (snap.bytes_tx_total / 1024 / 1024).toFixed(2) + ' MB' },
          { label: 'RX Total', value: (snap.bytes_rx_total / 1024 / 1024).toFixed(2) + ' MB' },
        ].map(({ label, value }) => (
          <div key={label} style={{ fontSize: 13 }}>
            <span style={{ color: '#8b949e' }}>{label}: </span>
            <span style={{ color: '#e6edf3', fontFamily: 'monospace' }}>{value}</span>
          </div>
        ))}
      </div>

      <div style={{ fontSize: 12, color: '#484f58' }}>
        Config: {result.config.test_type.toUpperCase()} · {result.config.associations.length} association(s) ·{' '}
        {[...new Set(result.config.associations.map((a) => a.protocol.toUpperCase()))].join('/')}
      </div>
    </div>
  )
}

export default function Results() {
  const { testResults, deleteResult, fetchResults } = useTestStore()
  const [expanded, setExpanded] = useState<string | null>(null)

  const toggle = (id: string) => setExpanded((prev) => (prev === id ? null : id))

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* 헤더 */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h2 style={{ fontSize: 16, fontWeight: 600 }}>
          Test Results
          <span style={{ color: '#8b949e', fontWeight: 400, fontSize: 13, marginLeft: 8 }}>
            ({testResults.length} stored)
          </span>
        </h2>
        <div style={{ display: 'flex', gap: 8 }}>
          <button className="btn-secondary" onClick={fetchResults} style={{ fontSize: 12, padding: '6px 12px' }}>
            Refresh
          </button>
          {testResults.length > 0 && (
            <button
              className="btn-secondary"
              onClick={() => downloadCsv(testResults)}
              style={{ fontSize: 12, padding: '6px 12px' }}
            >
              Export CSV
            </button>
          )}
        </div>
      </div>

      {testResults.length === 0 ? (
        <div className="card" style={{ color: '#8b949e', fontSize: 14, textAlign: 'center', padding: 40 }}>
          No test results yet. Run a test to see results here.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {testResults.map((r) => {
            const succ = successRate(r)
            const isOpen = expanded === r.id

            return (
              <div
                key={r.id}
                className="card"
                style={{ padding: 0, overflow: 'hidden' }}
              >
                {/* 행 헤더 */}
                <div
                  onClick={() => toggle(r.id)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 12,
                    padding: '12px 16px',
                    cursor: 'pointer',
                    background: isOpen ? '#1c2128' : 'transparent',
                    transition: 'background 0.15s',
                  }}
                >
                  <span style={{ fontSize: 11, color: '#8b949e', minWidth: 12 }}>{isOpen ? '▼' : '▶'}</span>

                  {/* 배지 */}
                  <span
                    style={{
                      padding: '2px 8px',
                      borderRadius: 20,
                      fontSize: 11,
                      fontWeight: 700,
                      background: r.config.test_type === 'cps' ? '#3fb950'
                        : r.config.test_type === 'bw' ? '#d29922' : '#58a6ff',
                      color: '#0d1117',
                      flexShrink: 0,
                    }}
                  >
                    {r.config.test_type.toUpperCase()}
                  </span>

                  {/* 이름 + 날짜 */}
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: 600, fontSize: 14, marginBottom: 2, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {r.config.name}
                    </div>
                    <div style={{ fontSize: 11, color: '#8b949e' }}>
                      {formatDate(r.started_at_secs)} · {formatTime(r.elapsed_secs)}
                    </div>
                  </div>

                  {/* 핵심 지표 요약 */}
                  <div style={{ display: 'flex', gap: 20, fontSize: 12, flexShrink: 0 }}>
                    <div>
                      <span style={{ color: '#8b949e' }}>CPS </span>
                      <span style={{ color: '#3fb950', fontWeight: 700, fontFamily: 'monospace' }}>
                        {r.final_snapshot.cps.toFixed(1)}
                      </span>
                    </div>
                    <div>
                      <span style={{ color: '#8b949e' }}>p99 </span>
                      <span style={{ color: '#f85149', fontWeight: 700, fontFamily: 'monospace' }}>
                        {r.final_snapshot.latency_p99_ms.toFixed(1)}ms
                      </span>
                    </div>
                    <div>
                      <span style={{ color: '#8b949e' }}>succ </span>
                      <span
                        style={{
                          color: succ >= 99 ? '#3fb950' : succ >= 90 ? '#d29922' : '#f85149',
                          fontWeight: 700,
                          fontFamily: 'monospace',
                        }}
                      >
                        {succ.toFixed(1)}%
                      </span>
                    </div>
                  </div>

                  {/* 액션 버튼 */}
                  <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}
                    onClick={(e) => e.stopPropagation()}>
                    <button
                      className="btn-secondary"
                      onClick={() => downloadJson(r)}
                      style={{ fontSize: 11, padding: '4px 8px' }}
                    >
                      JSON
                    </button>
                    <button
                      className="btn-danger"
                      onClick={() => deleteResult(r.id)}
                      style={{ fontSize: 11, padding: '4px 8px' }}
                    >
                      Delete
                    </button>
                  </div>
                </div>

                {/* 상세 펼침 */}
                {isOpen && (
                  <div style={{ borderTop: '1px solid #21262d', padding: '0 16px' }}>
                    <ResultDetail result={r} />
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
