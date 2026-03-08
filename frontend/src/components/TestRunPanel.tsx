import { useState } from 'react'
import { Play, StopCircle } from 'lucide-react'
import { useTestStore } from '../store/testStore'
import { Card, CardContent, CardTitle } from './ui/card'
import { Button } from './ui/button'
import { Badge } from './ui/badge'
import { NativeSelect } from './ui/input'
import { cn } from '@/lib/utils'
import { formatTime } from '../App'

export type ChartKey = 'cpsRps' | 'activeConn' | 'bandwidth' | 'latency' | 'histogram' | 'errors'

export const ALL_CHARTS: ChartKey[] = [
  'cpsRps', 'activeConn', 'bandwidth', 'latency', 'histogram', 'errors',
]

const CHART_LABELS: Record<ChartKey, string> = {
  cpsRps:     'CPS / RPS',
  activeConn: 'Active Conn',
  bandwidth:  'Bandwidth',
  latency:    'Latency',
  histogram:  'Histogram',
  errors:     'Errors',
}

const STATE_VARIANT: Record<string, Parameters<typeof Badge>[0]['variant']> = {
  idle:        'secondary',
  preparing:   'warning',
  ramping_up:  'purple',
  running:     'success',
  stopping:    'warning',
  completed:   'default',
  failed:      'destructive',
}

// A-3: 토글 버튼 그룹
function ChartToggleGroup({
  visibleCharts,
  onToggle,
}: {
  visibleCharts: Set<ChartKey>
  onToggle: (key: ChartKey) => void
}) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {ALL_CHARTS.map((key) => {
        const active = visibleCharts.has(key)
        return (
          <button
            key={key}
            onClick={() => onToggle(key)}
            className={cn(
              'px-3 py-1 rounded-md text-xs font-medium border transition-colors',
              active
                ? 'bg-primary text-primary-foreground border-primary'
                : 'bg-transparent text-muted-foreground border-border hover:text-foreground hover:border-muted-foreground',
            )}
          >
            {CHART_LABELS[key]}
          </button>
        )
      })}
    </div>
  )
}

export default function TestRunPanel({
  visibleCharts,
  onToggleChart,
}: {
  visibleCharts: Set<ChartKey>
  onToggleChart: (key: ChartKey) => void
}) {
  const { testState, activeProfile, elapsedSecs, savedProfiles, startTest, stopTest } = useTestStore()
  const [selectedProfileId, setSelectedProfileId] = useState<string>('')

  const isRunning = ['running', 'preparing', 'stopping', 'ramping_up'].includes(testState)
  const isIdle = ['idle', 'completed', 'failed'].includes(testState)

  const duration = activeProfile?.duration_secs ?? 0
  const elapsed = elapsedSecs ?? 0
  const remaining = duration > 0 ? Math.max(0, duration - elapsed) : null
  const progress = duration > 0 ? Math.min(100, (elapsed / duration) * 100) : 0

  const handleStart = () => {
    const profile = savedProfiles.find((p) => p.id === selectedProfileId)
    if (profile) startTest(profile)
  }

  return (
    <div className="flex flex-col gap-3">
      {/* 시험 상태 */}
      <Card>
        <CardContent className="flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <CardTitle>Test Status</CardTitle>
            <Badge variant={STATE_VARIANT[testState] ?? 'secondary'}>
              {testState.replace('_', ' ')}
            </Badge>
          </div>

          {activeProfile ? (
            <div className="flex flex-col gap-1">
              <div className="text-sm font-semibold text-foreground leading-tight">{activeProfile.name}</div>
              <div className="text-xs text-muted-foreground">
                {activeProfile.test_type.toUpperCase()} · {activeProfile.associations.length} association(s)
              </div>
            </div>
          ) : (
            <div className="text-xs text-muted-foreground italic">No active test</div>
          )}

          {elapsedSecs != null && (
            <div className="flex flex-col gap-2">
              <div className="flex justify-between text-sm">
                <span className="font-mono font-semibold text-foreground tabular-nums">{formatTime(elapsed)}</span>
                {remaining != null && (
                  <span className="font-mono text-muted-foreground tabular-nums">-{formatTime(remaining)}</span>
                )}
              </div>
              {duration > 0 && (
                <div className="h-1.5 bg-muted rounded-full overflow-hidden">
                  <div
                    className={cn(
                      'h-full rounded-full transition-[width] duration-500 ease-linear',
                      isRunning ? 'bg-success' : 'bg-primary',
                    )}
                    style={{ width: `${progress}%` }}
                  />
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      {/* 시작/중지 */}
      <Card>
        <CardContent className="flex flex-col gap-3">
          <CardTitle>Control</CardTitle>

          {isIdle && (
            <>
              <div>
                <label className="text-xs font-medium text-muted-foreground block mb-1.5">Saved Profile</label>
                <NativeSelect
                  value={selectedProfileId}
                  onChange={(e) => setSelectedProfileId(e.target.value)}
                >
                  <option value="">— select profile —</option>
                  {savedProfiles.map((p) => (
                    <option key={p.id} value={p.id}>{p.name}</option>
                  ))}
                </NativeSelect>
              </div>
              <Button disabled={!selectedProfileId} onClick={handleStart} className="w-full">
                <Play className="h-4 w-4" />
                Start Test
              </Button>
            </>
          )}

          {isRunning && (
            <Button variant="destructive" onClick={stopTest} className="w-full">
              <StopCircle className="h-4 w-4" />
              Stop Test
            </Button>
          )}

          {testState === 'completed' && (
            <div className="flex items-center justify-center gap-2 py-2 text-sm text-success font-medium">
              <span className="h-5 w-5 rounded-full bg-success/15 flex items-center justify-center text-xs">✓</span>
              Test completed
            </div>
          )}

          {testState === 'failed' && (
            <div className="flex items-center justify-center gap-2 py-2 text-sm text-destructive font-medium">
              <span className="h-5 w-5 rounded-full bg-destructive/15 flex items-center justify-center text-xs">✕</span>
              Test failed
            </div>
          )}
        </CardContent>
      </Card>

      {/* A-3: Toggle button group */}
      <Card>
        <CardContent className="flex flex-col gap-3">
          <CardTitle>Display Charts</CardTitle>
          <ChartToggleGroup visibleCharts={visibleCharts} onToggle={onToggleChart} />
        </CardContent>
      </Card>
    </div>
  )
}
