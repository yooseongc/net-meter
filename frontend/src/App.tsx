import { useEffect, useState } from 'react'
import { useTestStore } from './store/testStore'
import Dashboard from './components/Dashboard'
import ProfileManager from './components/ProfileManager'

type Tab = 'dashboard' | 'profiles'

export default function App() {
  const [tab, setTab] = useState<Tab>('dashboard')
  const { connectWs, fetchStatus, fetchProfiles } = useTestStore()

  useEffect(() => {
    fetchStatus()
    fetchProfiles()
    connectWs()
  }, [])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', minHeight: '100vh' }}>
      <header style={styles.header}>
        <span style={styles.logo}>net-meter</span>
        <nav style={styles.nav}>
          <button
            style={tab === 'dashboard' ? styles.tabActive : styles.tab}
            onClick={() => setTab('dashboard')}
          >
            Dashboard
          </button>
          <button
            style={tab === 'profiles' ? styles.tabActive : styles.tab}
            onClick={() => setTab('profiles')}
          >
            Profiles
          </button>
        </nav>
        <WsIndicator />
      </header>

      <main style={styles.main}>
        {tab === 'dashboard' && <Dashboard />}
        {tab === 'profiles' && <ProfileManager />}
      </main>
    </div>
  )
}

function WsIndicator() {
  const wsConnected = useTestStore((s) => s.wsConnected)
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12 }}>
      <div
        style={{
          width: 8,
          height: 8,
          borderRadius: '50%',
          background: wsConnected ? '#3fb950' : '#f85149',
        }}
      />
      <span style={{ color: '#8b949e' }}>{wsConnected ? 'Live' : 'Disconnected'}</span>
    </div>
  )
}

const styles: Record<string, React.CSSProperties> = {
  header: {
    display: 'flex',
    alignItems: 'center',
    gap: 24,
    padding: '12px 24px',
    borderBottom: '1px solid #30363d',
    background: '#161b22',
  },
  logo: {
    fontWeight: 700,
    fontSize: 18,
    color: '#58a6ff',
    letterSpacing: '-0.02em',
  },
  nav: {
    display: 'flex',
    gap: 4,
    flex: 1,
  },
  tab: {
    background: 'transparent',
    color: '#8b949e',
    padding: '6px 12px',
    borderRadius: 6,
    fontSize: 14,
  },
  tabActive: {
    background: '#21262d',
    color: '#e6edf3',
    padding: '6px 12px',
    borderRadius: 6,
    fontSize: 14,
  },
  main: {
    flex: 1,
    padding: 24,
    maxWidth: 1400,
    margin: '0 auto',
    width: '100%',
  },
}
