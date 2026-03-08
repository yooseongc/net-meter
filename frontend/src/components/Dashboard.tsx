import { useState } from 'react'
import { useTestStore } from '../store/testStore'
import MetricsPanel from './MetricsPanel'
import EventLog from './EventLog'
import TestRunPanel, { ALL_CHARTS, ChartKey } from './TestRunPanel'
import TopologyView from './TopologyView'

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
    <div className="flex flex-col gap-4">
      {error && (
        <div className="bg-destructive/10 border border-destructive rounded-xl px-4 py-2.5 text-sm text-destructive">
          <strong>Error:</strong> {error}
        </div>
      )}

      {/* 상단: 토폴로지 + 이벤트 로그 (시각적으로 연결) */}
      <div className="flex flex-col gap-2">
        <TopologyView compact />
        <EventLog />
      </div>

      {/* 하단: 시험 제어 | 차트 */}
      <div className="grid gap-4 items-start" style={{ gridTemplateColumns: '280px 1fr' }}>
        <TestRunPanel visibleCharts={visibleCharts} onToggleChart={onToggleChart} />
        <MetricsPanel visibleCharts={visibleCharts} />
      </div>
    </div>
  )
}
