import { useEffect, useState } from 'react'
import { useTestStore } from './store/testStore'
import Dashboard from './components/Dashboard'
import TestControl from './components/TestControl'
import TopologyView from './components/TopologyView'
import ProfileManager from './components/ProfileManager'
import Results from './components/Results'

type Tab = 'monitor' | 'config' | 'topology' | 'profiles' | 'results'

export default function App() {
  const [tab, setTab] = useState<Tab>('monitor')
  const { connectWs, fetchStatus, fetchProfiles, fetchResults } = useTestStore()

  useEffect(() => {
    fetchStatus()
    fetchProfiles()
    fetchResults()
    connectWs()
  }, [])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', minHeight: '100vh' }}>
      <Header tab={tab} setTab={setTab} />
      <main style={styles.main}>
        {tab === 'monitor' && <Dashboard />}
        {tab === 'config' && <TestControl />}
        {tab === 'topology' && <TopologyView />}
        {tab === 'profiles' && <ProfileManager />}
        {tab === 'results' && <Results />}
      </main>
    </div>
  )
}

function Header({ tab, setTab }: { tab: Tab; setTab: (t: Tab) => void }) {
  const { testState, activeProfile, elapsedSecs, stopTest, wsConnected } =
    useTestStore()

  const isRunning =
    testState === 'running' || testState === 'preparing' || testState === 'stopping' || testState === 'ramping_up'

  const { latestSnapshot, eventLog } = useTestStore()
  const hasViolations = (latestSnapshot?.threshold_violations?.length ?? 0) > 0
  const unreadWarnings = eventLog.filter(e => e.level === 'warn' || e.level === 'error').length

  const tabs: { id: Tab; label: string }[] = [
    { id: 'monitor', label: 'Monitor' },
    { id: 'config', label: 'Config' },
    { id: 'topology', label: 'Topology' },
    { id: 'profiles', label: 'Profiles' },
    { id: 'results', label: 'Results' },
  ]

  const duration = activeProfile?.duration_secs ?? 0
  const remaining = duration > 0 && elapsedSecs != null ? Math.max(0, duration - elapsedSecs) : null

  return (
    <header style={styles.header}>
      {/* 로고 */}
      <span style={styles.logo}>net-meter</span>

      {/* 탭 내비게이션 */}
      <nav style={styles.nav}>
        {tabs.map(({ id, label }) => (
          <button
            key={id}
            style={tab === id ? styles.tabActive : styles.tab}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </nav>

      {/* 시험 상태 정보 */}
      {activeProfile && (
        <div style={styles.testInfo}>
          <StateBadge state={testState} />
          <span style={{ color: '#8b949e', fontSize: 12 }}>
            {activeProfile.name}
          </span>
          {elapsedSecs != null && (
            <span style={{ color: '#e6edf3', fontSize: 12, fontFamily: 'monospace' }}>
              {formatTime(elapsedSecs)}
              {remaining != null && (
                <span style={{ color: '#8b949e' }}> / -{formatTime(remaining)}</span>
              )}
            </span>
          )}
          {duration > 0 && elapsedSecs != null && (
            <div style={styles.progressBar}>
              <div
                style={{
                  ...styles.progressFill,
                  width: `${Math.min(100, (elapsedSecs / duration) * 100)}%`,
                  background: isRunning ? '#3fb950' : '#8b949e',
                }}
              />
            </div>
          )}
        </div>
      )}

      {/* 글로벌 Start/Stop */}
      <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
        {isRunning && (
          <button className="btn-danger" onClick={stopTest} style={{ padding: '6px 14px', fontSize: 13 }}>
            Stop
          </button>
        )}

        {/* 임계값 위반 알람 */}
        {hasViolations && (
          <div style={{
            display: 'flex', alignItems: 'center', gap: 5, fontSize: 11,
            background: '#2d1515', border: '1px solid #f85149', borderRadius: 6,
            padding: '3px 8px', color: '#f85149', fontWeight: 700,
            animation: 'pulse 1.5s ease-in-out infinite',
          }}>
            ⚠ Threshold Violation
          </div>
        )}

        {/* 이벤트 경고 카운트 */}
        {unreadWarnings > 0 && !hasViolations && (
          <div style={{
            display: 'flex', alignItems: 'center', gap: 4, fontSize: 11,
            background: '#2d2015', border: '1px solid #d29922', borderRadius: 6,
            padding: '3px 8px', color: '#d29922',
          }}>
            ⚠ {unreadWarnings}
          </div>
        )}

        {/* WS 연결 상태 */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12 }}>
          <div
            style={{
              width: 7,
              height: 7,
              borderRadius: '50%',
              background: wsConnected ? '#3fb950' : '#f85149',
            }}
          />
          <span style={{ color: '#8b949e' }}>{wsConnected ? 'Live' : 'Disconnected'}</span>
        </div>
      </div>
    </header>
  )
}

function StateBadge({ state }: { state: string }) {
  const colors: Record<string, string> = {
    idle: '#8b949e',
    preparing: '#d29922',
    ramping_up: '#bc8cff',
    running: '#3fb950',
    stopping: '#d29922',
    completed: '#58a6ff',
    failed: '#f85149',
  }
  return (
    <span
      style={{
        padding: '2px 8px',
        borderRadius: 20,
        fontSize: 11,
        fontWeight: 700,
        background: colors[state] ?? '#8b949e',
        color: '#0d1117',
        textTransform: 'uppercase',
        flexShrink: 0,
      }}
    >
      {state}
    </span>
  )
}

function formatTime(secs: number): string {
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${m}:${String(s).padStart(2, '0')}`
}

const styles: Record<string, React.CSSProperties> = {
  header: {
    display: 'flex',
    alignItems: 'center',
    gap: 16,
    padding: '10px 20px',
    borderBottom: '1px solid #30363d',
    background: '#161b22',
    flexWrap: 'wrap',
  },
  logo: {
    fontWeight: 700,
    fontSize: 16,
    color: '#58a6ff',
    letterSpacing: '-0.02em',
    flexShrink: 0,
  },
  nav: {
    display: 'flex',
    gap: 2,
  },
  tab: {
    background: 'transparent',
    color: '#8b949e',
    padding: '5px 12px',
    borderRadius: 6,
    fontSize: 13,
  },
  tabActive: {
    background: '#21262d',
    color: '#e6edf3',
    padding: '5px 12px',
    borderRadius: 6,
    fontSize: 13,
  },
  testInfo: {
    display: 'flex',
    alignItems: 'center',
    gap: 8,
    flex: 1,
    minWidth: 0,
  },
  progressBar: {
    width: 80,
    height: 4,
    background: '#21262d',
    borderRadius: 2,
    overflow: 'hidden',
    flexShrink: 0,
  },
  progressFill: {
    height: '100%',
    borderRadius: 2,
    transition: 'width 0.5s linear',
  },
  main: {
    flex: 1,
    padding: '20px 24px',
    maxWidth: 1600,
    margin: '0 auto',
    width: '100%',
  },
}
