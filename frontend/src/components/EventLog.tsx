import { useTestStore } from '../store/testStore'
import { Card, CardContent, CardTitle } from './ui/card'
import { Button } from './ui/button'
import { cn } from '@/lib/utils'

export default function EventLog() {
  const { eventLog, clearEventLog } = useTestStore()

  return (
    <Card>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <CardTitle>Event Log</CardTitle>
          {eventLog.length > 0 && (
            <Button variant="ghost" size="xs" onClick={clearEventLog} className="text-muted-foreground">
              Clear
            </Button>
          )}
        </div>

        <div className="h-28 overflow-y-auto flex flex-col gap-px">
          {eventLog.length === 0 ? (
            <div className="h-full flex items-center justify-center text-xs text-muted-foreground/50 italic select-none">
              No events yet
            </div>
          ) : (
            eventLog.map((entry) => (
              <div
                key={entry.id}
                className={cn(
                  'flex gap-2 px-2 py-1 rounded-md text-xs',
                  entry.level === 'error' && 'bg-destructive/8',
                  entry.level === 'warn' && 'bg-warning/8',
                )}
              >
                <span className="text-muted-foreground/50 shrink-0 font-mono">{entry.ts}</span>
                <span
                  className={cn(
                    'shrink-0 font-semibold font-mono w-9',
                    entry.level === 'error' ? 'text-destructive'
                      : entry.level === 'warn' ? 'text-warning'
                      : 'text-muted-foreground',
                  )}
                >
                  {entry.level === 'error' ? 'ERR' : entry.level === 'warn' ? 'WRN' : 'INF'}
                </span>
                <span
                  className={cn(
                    'leading-relaxed',
                    entry.level === 'error' ? 'text-destructive'
                      : entry.level === 'warn' ? 'text-warning'
                      : 'text-foreground/80',
                  )}
                >
                  {entry.message}
                </span>
              </div>
            ))
          )}
        </div>
      </CardContent>
    </Card>
  )
}
