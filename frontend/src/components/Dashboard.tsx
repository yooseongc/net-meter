import { useTestStore } from '../store/testStore'
import TestControl from './TestControl'
import MetricsPanel from './MetricsPanel'

export default function Dashboard() {
  const { error } = useTestStore()

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {error && (
        <div style={styles.error}>
          <strong>Error:</strong> {error}
        </div>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: '340px 1fr', gap: 16, alignItems: 'start' }}>
        <TestControl />
        <MetricsPanel />
      </div>
    </div>
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
