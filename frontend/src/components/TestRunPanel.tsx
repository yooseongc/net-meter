import { useState } from 'react'
import { useTestStore } from '../store/testStore'

export type ChartKey = 'cpsRps' | 'activeConn' | 'bandwidth' | 'latency' | 'histogram' | 'errors'

export const ALL_CHARTS: ChartKey[] = [
  'cpsRps', 'activeConn', 'bandwidth', 'latency', 'histogram', 'errors',
]

const CHART_LABELS: Record<ChartKey, string> = {
  cpsRps: 'CPS / RPS',
  activeConn: 'Active Connections',
  bandwidth: 'Bandwidth',
  latency: 'Latency Timeline',
  histogram: 'Latency Histogram',
  errors: 'Error Breakdown',
}

const STATE_COLORS: Record<string, string> = {
  idle: '#8b949e',
  preparing: '#d29922',
  ramping_up: '#bc8cff',
  running: '#3fb950',
  stopping: '#d29922',
  completed: '#58a6ff',
  failed: '#f85149',
}

function formatTime(secs: number): string {
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${m}:${String(s).padStart(2, '0')}`
}

export default function TestRunPanel({
  visibleCharts,
  onToggleChart,
}: {
  visibleCharts: Set<ChartKey>
  onToggleChart: (key: ChartKey) => void
}) {
  const {
    testState, activeProfile, elapsedSecs,
    savedProfiles, startTest, stopTest,
  } = useTestStore()

  const [selectedProfileId, setSelectedProfileId] = useState<string>('')

  const isRunning = testState === 'running' || testState === 'preparing'
    || testState === 'stopping' || testState === 'ramping_up'
  const isIdle = testState === 'idle' || testState === 'completed' || testState === 'failed'

  const duration = activeProfile?.duration_secs ?? 0
  const elapsed = elapsedSecs ?? 0
  const remaining = duration > 0 ? Math.max(0, duration - elapsed) : null
  const progress = duration > 0 ? Math.min(100, (elapsed / duration) * 100) : 0

  const handleStart = () => {
    const profile = savedProfiles.find((p) => p.id === selectedProfileId)
    if (profile) startTest(profile)
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>

      {/* 시험 상태 */}
      <div className="card" style={{ gap: 10 }}>
        <div className="card-title">Test Status</div>

        <span style={{
          alignSelf: 'flex-start',
          padding: '2px 10px', borderRadius: 20, fontSize: 11, fontWeight: 700,
          background: STATE_COLORS[testState] ?? '#8b949e', color: '#0d1117',
          textTransform: 'uppercase',
        }}>
          {testState.replace('_', ' ')}
        </span>

        {activeProfile && (
          <div style={{ fontSize: 12, color: '#e6edf3', fontWeight: 600 }}>
            {activeProfile.name}
          </div>
        )}
        {activeProfile && (
          <div style={{ fontSize: 11, color: '#8b949e' }}>
            {activeProfile.test_type.toUpperCase()} · {activeProfile.pairs.length} pair(s)
          </div>
        )}

        {/* 경과 / 남은 시간 */}
        {elapsedSecs != null && (
          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12, marginBottom: 4 }}>
              <span style={{ color: '#e6edf3', fontFamily: 'monospace' }}>
                {formatTime(elapsed)}
              </span>
              {remaining != null && (
                <span style={{ color: '#8b949e', fontFamily: 'monospace' }}>
                  -{formatTime(remaining)}
                </span>
              )}
            </div>
            {duration > 0 && (
              <div style={{ height: 4, background: '#21262d', borderRadius: 2, overflow: 'hidden' }}>
                <div style={{
                  height: '100%', width: `${progress}%`,
                  background: isRunning ? '#3fb950' : '#58a6ff',
                  borderRadius: 2, transition: 'width 0.5s linear',
                }} />
              </div>
            )}
          </div>
        )}
      </div>

      {/* 시작/중지 컨트롤 */}
      <div className="card" style={{ gap: 10 }}>
        <div className="card-title">Control</div>

        {isIdle && (
          <>
            <div>
              <label style={{ fontSize: 11, color: '#8b949e', display: 'block', marginBottom: 4 }}>
                Saved Profile
              </label>
              <select
                value={selectedProfileId}
                onChange={(e) => setSelectedProfileId(e.target.value)}
              >
                <option value="">— select —</option>
                {savedProfiles.map((p) => (
                  <option key={p.id} value={p.id}>{p.name}</option>
                ))}
              </select>
            </div>
            <button
              className="btn-primary"
              disabled={!selectedProfileId}
              onClick={handleStart}
              style={{ width: '100%', padding: '8px 0', fontSize: 13 }}
            >
              Start Test
            </button>
          </>
        )}

        {isRunning && (
          <button
            className="btn-danger"
            onClick={stopTest}
            style={{ width: '100%', padding: '8px 0', fontSize: 13 }}
          >
            Stop Test
          </button>
        )}

        {testState === 'completed' && (
          <div style={{ fontSize: 12, color: '#3fb950', textAlign: 'center', padding: '4px 0' }}>
            ✓ Test completed
          </div>
        )}
      </div>

      {/* 표시할 차트 선택 */}
      <div className="card" style={{ gap: 8 }}>
        <div className="card-title">Display</div>
        {ALL_CHARTS.map((key) => (
          <label key={key} style={{
            display: 'flex', alignItems: 'center', gap: 8,
            cursor: 'pointer', fontSize: 12, color: '#8b949e',
          }}>
            <input
              type="checkbox"
              checked={visibleCharts.has(key)}
              onChange={() => onToggleChart(key)}
              style={{ accentColor: '#58a6ff' }}
            />
            {CHART_LABELS[key]}
          </label>
        ))}
      </div>

    </div>
  )
}
