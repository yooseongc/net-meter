import { useEffect, useState } from 'react'
import { Sun, Moon, Monitor, Wifi, WifiOff, Square, TriangleAlert } from 'lucide-react'
import { useTestStore } from './store/testStore'
import { useTheme } from './lib/theme'
import { cn } from './lib/utils'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import Dashboard from './components/Dashboard'
import TestControl from './components/TestControl'
import ProfileManager from './components/ProfileManager'
import Results from './components/Results'

type Tab = 'monitor' | 'config' | 'profiles' | 'results'

const TABS: { id: Tab; label: string }[] = [
  { id: 'monitor',  label: 'Monitor'  },
  { id: 'config',   label: 'Config'   },
  { id: 'profiles', label: 'Profiles' },
  { id: 'results',  label: 'Results'  },
]

export default function App() {
  const [tab, setTab] = useState<Tab>('monitor')
  const { connectWs, fetchStatus, fetchProfiles, fetchResults, setDraftConfig } = useTestStore()

  useEffect(() => {
    fetchStatus()
    fetchProfiles()
    fetchResults()
    connectWs()
  }, [])

  const handleLoadConfig = (c: NonNullable<Parameters<typeof setDraftConfig>[0]>) => {
    setDraftConfig(c)
    setTab('config')
  }

  return (
    <div className="flex flex-col min-h-screen bg-background text-foreground">
      {/* ── 헤더 ── */}
      <Header onStop={() => {}} />

      {/* ── 탭 바 ── */}
      <div className="bg-header border-b border-border flex-shrink-0">
        <div className="flex px-4">
          {TABS.map(({ id, label }) => (
            <button
              key={id}
              onClick={() => setTab(id)}
              className={cn(
                'px-6 py-3 text-sm font-medium border-b-2 transition-colors focus:outline-none',
                tab === id
                  ? 'border-primary text-primary'
                  : 'border-transparent text-muted-foreground hover:text-foreground',
              )}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {/* ── 콘텐츠 ── */}
      <main className="flex-1 overflow-y-auto">
        <div className="p-6 max-w-[1600px] mx-auto w-full">
          {tab === 'monitor' && <Dashboard />}
          {/* TestControl은 항상 마운트 상태 유지 (자동저장 + 상태 보존) */}
          <div style={{ display: tab === 'config' ? 'block' : 'none' }}>
            <TestControl />
          </div>
          {tab === 'profiles' && <ProfileManager onLoadConfig={handleLoadConfig} />}
          {tab === 'results' && <Results />}
        </div>
      </main>
    </div>
  )
}

// ─── 헤더 ────────────────────────────────────────────────────────────────────

function ThemeToggle() {
  const { theme, setTheme } = useTheme()
  const next: typeof theme = theme === 'dark' ? 'light' : theme === 'light' ? 'system' : 'dark'
  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={() => setTheme(next)}
      title={`Current theme: ${theme}`}
      className="h-9 w-9 text-muted-foreground hover:text-foreground"
    >
      {theme === 'dark'   ? <Moon className="h-4 w-4" />
        : theme === 'light' ? <Sun className="h-4 w-4" />
        : <Monitor className="h-4 w-4" />}
    </Button>
  )
}

// 시험 실행 중 헤더에 표시되는 컴팩트 상태 위젯
function ActiveTestStatus() {
  const { testState, activeProfile, elapsedSecs } = useTestStore()
  const isActive = ACTIVE_STATES.includes(testState)
  if (!isActive && testState !== 'completed' && testState !== 'failed') return null
  if (!activeProfile) return null

  const duration = activeProfile.duration_secs
  const elapsed = elapsedSecs ?? 0
  const remaining = duration > 0 ? Math.max(0, duration - elapsed) : null
  const progress = duration > 0 ? Math.min(100, (elapsed / duration) * 100) : 0

  return (
    <div className="flex items-center gap-3 px-3 py-1.5 rounded-lg bg-muted border border-border text-sm max-w-[460px]">
      <StateBadge state={testState} />
      <span className="font-medium truncate max-w-[140px] text-foreground">{activeProfile.name}</span>
      <span className="font-mono text-foreground tabular-nums">{formatTime(elapsed)}</span>
      {remaining != null && (
        <span className="font-mono text-muted-foreground tabular-nums">-{formatTime(remaining)}</span>
      )}
      {duration > 0 && (
        <div className="w-20 h-1.5 bg-border rounded-full overflow-hidden shrink-0">
          <div
            className={cn(
              'h-full rounded-full transition-[width] duration-500 ease-linear',
              isActive ? 'bg-success' : 'bg-muted-foreground',
            )}
            style={{ width: `${progress}%` }}
          />
        </div>
      )}
    </div>
  )
}

const ACTIVE_STATES = ['preparing', 'ramping_up', 'running', 'stopping']

function Header({ onStop: _onStop }: { onStop: () => void }) {
  const { testState, stopTest, wsConnected, latestSnapshot, eventLog } = useTestStore()

  const isRunning = ACTIVE_STATES.includes(testState)
  const hasViolations = (latestSnapshot?.threshold_violations?.length ?? 0) > 0
  const unreadWarnings = eventLog.filter(e => e.level === 'warn' || e.level === 'error').length

  return (
    <header className="flex items-center px-6 h-14 bg-header flex-shrink-0" style={{ boxShadow: 'var(--header-shadow)' }}>
      {/* Logo */}
      <div className="flex items-center gap-2 mr-6 shrink-0">
        <span className="font-bold text-lg text-primary tracking-tight">net-meter</span>
      </div>

      {/* Active test status */}
      <ActiveTestStatus />

      <div className="flex-1" />

      {/* Right actions */}
      <div className="flex items-center gap-3">
        {isRunning && (
          <Button variant="destructive" size="sm" onClick={stopTest}>
            <Square className="h-3.5 w-3.5 fill-current" />
            Stop
          </Button>
        )}

        {hasViolations && (
          <div className="flex items-center gap-2 text-destructive bg-destructive/10 border border-destructive/30 rounded-lg px-4 py-1.5 text-xs font-semibold">
            <TriangleAlert className="h-3.5 w-3.5 shrink-0" />
            Threshold Violation
          </div>
        )}

        {unreadWarnings > 0 && !hasViolations && (
          <div className="flex items-center gap-2 text-warning bg-warning/10 border border-warning/30 rounded-lg px-4 py-1.5 text-xs font-semibold">
            <TriangleAlert className="h-3.5 w-3.5 shrink-0" />
            {unreadWarnings} warning{unreadWarnings > 1 ? 's' : ''}
          </div>
        )}

        <div className="flex items-center gap-2 text-xs text-muted-foreground border-l border-border pl-3">
          {wsConnected
            ? <Wifi className="h-4 w-4 text-success shrink-0" />
            : <WifiOff className="h-4 w-4 text-destructive shrink-0" />}
          <span className="font-medium">{wsConnected ? 'Live' : 'Disconnected'}</span>
        </div>

        <ThemeToggle />
      </div>
    </header>
  )
}

// ─── 공유 컴포넌트 ────────────────────────────────────────────────────────────

const STATE_VARIANT: Record<string, Parameters<typeof Badge>[0]['variant']> = {
  idle:        'secondary',
  preparing:   'warning',
  ramping_up:  'purple',
  running:     'success',
  stopping:    'warning',
  completed:   'default',
  failed:      'destructive',
}

export function StateBadge({ state }: { state: string }) {
  return (
    <Badge variant={STATE_VARIANT[state] ?? 'secondary'}>
      {state.replace('_', ' ')}
    </Badge>
  )
}

export function formatTime(secs: number): string {
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${m}:${String(s).padStart(2, '0')}`
}
