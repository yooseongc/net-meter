import { useTestStore } from '../store/testStore'

export default function EventLog() {
  const { eventLog, clearEventLog } = useTestStore()

  if (eventLog.length === 0) return null

  return (
    <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div className="card-title" style={{ margin: 0 }}>Event Log</div>
        <button
          className="btn-secondary"
          onClick={clearEventLog}
          style={{ fontSize: 11, padding: '3px 8px' }}
        >
          Clear
        </button>
      </div>

      <div
        style={{
          maxHeight: 200,
          overflowY: 'auto',
          display: 'flex',
          flexDirection: 'column',
          gap: 2,
          fontFamily: 'monospace',
          fontSize: 12,
        }}
      >
        {eventLog.map((entry) => (
          <div
            key={entry.id}
            style={{
              display: 'flex',
              gap: 10,
              padding: '2px 4px',
              borderRadius: 3,
              background: entry.level === 'error'
                ? 'rgba(248,81,73,0.08)'
                : entry.level === 'warn'
                ? 'rgba(210,153,34,0.08)'
                : 'transparent',
            }}
          >
            <span style={{ color: '#484f58', flexShrink: 0 }}>{entry.ts}</span>
            <span
              style={{
                color: entry.level === 'error' ? '#f85149'
                  : entry.level === 'warn' ? '#d29922'
                  : '#8b949e',
                flexShrink: 0,
                width: 36,
              }}
            >
              [{entry.level.toUpperCase().slice(0, 4)}]
            </span>
            <span style={{ color: entry.level === 'error' ? '#f85149'
              : entry.level === 'warn' ? '#e6ac3a' : '#e6edf3' }}>
              {entry.message}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}
