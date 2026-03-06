import { useTestStore } from '../store/testStore'
import TestControl from './TestControl'
import MetricsPanel from './MetricsPanel'

export default function Dashboard() {
  const { testState, activeProfile, error } = useTestStore()

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      {error && (
        <div style={styles.error}>
          <strong>Error:</strong> {error}
        </div>
      )}

      {/* 상태 배너 */}
      <div className="card" style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
        <StateBadge state={testState} />
        {activeProfile && (
          <span style={{ color: '#8b949e', fontSize: 14 }}>
            {activeProfile.name} &mdash; {activeProfile.test_type.toUpperCase()} /&nbsp;
            {activeProfile.protocol.toUpperCase()} &mdash;&nbsp;
            {activeProfile.target_host}:{activeProfile.target_port}
          </span>
        )}
      </div>

      {/* 2단 레이아웃: 제어 패널 + 메트릭 */}
      <div style={{ display: 'grid', gridTemplateColumns: '320px 1fr', gap: 20 }}>
        <TestControl />
        <MetricsPanel />
      </div>
    </div>
  )
}

function StateBadge({ state }: { state: string }) {
  const colors: Record<string, string> = {
    idle: '#8b949e',
    preparing: '#d29922',
    running: '#3fb950',
    stopping: '#d29922',
    completed: '#58a6ff',
    failed: '#f85149',
  }
  return (
    <span
      style={{
        padding: '4px 10px',
        borderRadius: 20,
        fontSize: 12,
        fontWeight: 700,
        background: colors[state] ?? '#8b949e',
        color: '#0d1117',
        textTransform: 'uppercase',
      }}
    >
      {state}
    </span>
  )
}

const styles: Record<string, React.CSSProperties> = {
  error: {
    background: '#2d1515',
    border: '1px solid #f85149',
    borderRadius: 8,
    padding: '10px 16px',
    color: '#f85149',
    fontSize: 14,
  },
}
