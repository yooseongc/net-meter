import { useState } from 'react'
import { useTestStore } from '../store/testStore'
import MetricsPanel from './MetricsPanel'
import EventLog from './EventLog'
import TestRunPanel, { ALL_CHARTS, ChartKey } from './TestRunPanel'

export default function Dashboard() {
  const { error } = useTestStore()
  const [visibleCharts, setVisibleCharts] = useState<Set<ChartKey>>(new Set(ALL_CHARTS))

  const onToggleChart = (key: ChartKey) => {
    setVisibleCharts((prev) => {
      const next = new Set(prev)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      return next
    })
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      {error && (
        <div style={styles.error}>
          <strong>Error:</strong> {error}
        </div>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: '260px 1fr', gap: 16, alignItems: 'start' }}>
        <TestRunPanel visibleCharts={visibleCharts} onToggleChart={onToggleChart} />
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <MetricsPanel visibleCharts={visibleCharts} />
          <EventLog />
        </div>
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
