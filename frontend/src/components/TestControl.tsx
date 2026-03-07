import { useState } from 'react'
import { TestProfile, TestType, Protocol, HttpMethod } from '../api/client'
import { useTestStore } from '../store/testStore'
import { v4 as uuidv4 } from 'uuid'

const defaultProfile = (): TestProfile => ({
  id: uuidv4(),
  name: 'New Test',
  test_type: 'cps',
  protocol: 'http1',
  target_host: '127.0.0.1',
  target_port: 8080,
  duration_secs: 30,
  target_cps: 100,
  target_cc: undefined,
  request_body_bytes: undefined,
  response_body_bytes: undefined,
  method: 'GET',
  path: '/',
  connect_timeout_ms: 5000,
  response_timeout_ms: 30000,
  max_inflight: undefined,
  use_namespace: false,
  netns_prefix: 'nm',
  tcp_quickack: false,
  path_extra_bytes: undefined,
  num_clients: 1,
  num_servers: 1,
})

// 접기/펼치기 가능한 섹션
function Section({
  title,
  children,
  defaultOpen = true,
}: {
  title: string
  children: React.ReactNode
  defaultOpen?: boolean
}) {
  const [open, setOpen] = useState(defaultOpen)
  return (
    <div style={{ borderTop: '1px solid #21262d', paddingTop: 10 }}>
      <button
        onClick={() => setOpen(!open)}
        style={{
          background: 'none',
          color: '#8b949e',
          fontSize: 11,
          fontWeight: 700,
          textTransform: 'uppercase',
          letterSpacing: '0.08em',
          padding: '0 0 6px 0',
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          width: '100%',
          textAlign: 'left',
        }}
      >
        <span style={{ fontSize: 10, color: '#58a6ff' }}>{open ? '▼' : '▶'}</span>
        {title}
      </button>
      {open && <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>{children}</div>}
    </div>
  )
}

function Row({ children }: { children: React.ReactNode }) {
  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
      {children}
    </div>
  )
}

function Field({ label, unit, children }: { label: string; unit?: string; children: React.ReactNode }) {
  return (
    <div>
      <label style={{ display: 'flex', justifyContent: 'space-between' }}>
        <span>{label}</span>
        {unit && <span style={{ color: '#484f58', fontSize: 11 }}>{unit}</span>}
      </label>
      {children}
    </div>
  )
}

export default function TestControl() {
  const { testState, startTest, stopTest, savedProfiles } = useTestStore()
  const [profile, setProfile] = useState<TestProfile>(defaultProfile)

  const isRunning = testState === 'running' || testState === 'preparing' || testState === 'stopping'

  const set = <K extends keyof TestProfile>(key: K, value: TestProfile[K]) => {
    setProfile((prev) => ({ ...prev, [key]: value }))
  }

  const setNum = (key: keyof TestProfile, raw: string, allowEmpty = true) => {
    if (allowEmpty && raw === '') {
      setProfile((prev) => ({ ...prev, [key]: undefined }))
    } else {
      const n = Number(raw)
      if (!isNaN(n)) setProfile((prev) => ({ ...prev, [key]: n }))
    }
  }

  const loadProfile = (id: string) => {
    const p = savedProfiles.find((x) => x.id === id)
    if (p) setProfile({ ...p })
  }

  const exportProfile = () => {
    const blob = new Blob([JSON.stringify(profile, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `${profile.name.replace(/\s+/g, '_')}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  const importProfile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = (ev) => {
      try {
        const data = JSON.parse(ev.target?.result as string) as TestProfile
        setProfile({ ...data, id: uuidv4() })
      } catch {
        /* ignore */
      }
    }
    reader.readAsText(file)
    e.target.value = ''
  }

  return (
    <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div className="card-title" style={{ margin: 0 }}>Test Control</div>
        <div style={{ display: 'flex', gap: 6 }}>
          <button className="btn-secondary" onClick={exportProfile} style={{ padding: '4px 10px', fontSize: 11 }}>
            Export
          </button>
          <label
            className="btn-secondary"
            style={{ padding: '4px 10px', fontSize: 11, cursor: 'pointer', borderRadius: 6, border: '1px solid #30363d', background: '#21262d', color: '#e6edf3', fontWeight: 600 }}
          >
            Import
            <input type="file" accept=".json" onChange={importProfile} style={{ display: 'none' }} />
          </label>
        </div>
      </div>

      {/* 저장된 프로파일 불러오기 */}
      {savedProfiles.length > 0 && (
        <div>
          <label>Load Saved Profile</label>
          <select onChange={(e) => loadProfile(e.target.value)} defaultValue="">
            <option value="" disabled>Select…</option>
            {savedProfiles.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
        </div>
      )}

      {/* === BASIC === */}
      <Section title="Basic" defaultOpen={true}>
        <Field label="Profile Name">
          <input value={profile.name} onChange={(e) => set('name', e.target.value)} />
        </Field>
        <Row>
          <Field label="Test Type">
            <select value={profile.test_type} onChange={(e) => set('test_type', e.target.value as TestType)}>
              <option value="cps">CPS</option>
              <option value="bw">BW</option>
              <option value="cc">CC</option>
            </select>
          </Field>
          <Field label="Protocol">
            <select value={profile.protocol} onChange={(e) => set('protocol', e.target.value as Protocol)}>
              <option value="http1">HTTP/1.1</option>
              <option value="http2">HTTP/2</option>
            </select>
          </Field>
        </Row>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 80px', gap: 8 }}>
          <Field label="Target Host">
            <input value={profile.target_host} onChange={(e) => set('target_host', e.target.value)} />
          </Field>
          <Field label="Port">
            <input type="number" value={profile.target_port} onChange={(e) => set('target_port', Number(e.target.value))} />
          </Field>
        </div>
        <Field label="Duration" unit="sec (0=manual stop)">
          <input type="number" value={profile.duration_secs} onChange={(e) => set('duration_secs', Number(e.target.value))} />
        </Field>
      </Section>

      {/* === LOAD === */}
      <Section title="Load" defaultOpen={true}>
        {profile.test_type === 'cps' && (
          <Field label="Target CPS" unit="/s">
            <input type="number" value={profile.target_cps ?? 100}
              onChange={(e) => set('target_cps', Number(e.target.value))} />
          </Field>
        )}
        {(profile.test_type === 'cc' || profile.test_type === 'bw') && (
          <Field label="Target Concurrent Connections" unit="conn">
            <input type="number" value={profile.target_cc ?? 50}
              onChange={(e) => set('target_cc', Number(e.target.value))} />
          </Field>
        )}
        <Field label="Max In-flight" unit="conn (blank=auto)">
          <input type="number"
            value={profile.max_inflight ?? ''}
            placeholder="auto"
            onChange={(e) => setNum('max_inflight', e.target.value)} />
        </Field>
      </Section>

      {/* === HTTP === */}
      <Section title="HTTP" defaultOpen={false}>
        <Row>
          <Field label="Method">
            <select value={profile.method} onChange={(e) => set('method', e.target.value as HttpMethod)}>
              <option value="GET">GET</option>
              <option value="POST">POST</option>
            </select>
          </Field>
          <Field label="Path">
            <input value={profile.path} onChange={(e) => set('path', e.target.value)} />
          </Field>
        </Row>
        <Row>
          <Field label="Request Body" unit="bytes">
            <input type="number"
              value={profile.request_body_bytes ?? ''}
              placeholder="none"
              onChange={(e) => setNum('request_body_bytes', e.target.value)} />
          </Field>
          <Field label="Response Body" unit="bytes">
            <input type="number"
              value={profile.response_body_bytes ?? ''}
              placeholder="none"
              onChange={(e) => setNum('response_body_bytes', e.target.value)} />
          </Field>
        </Row>
        <Field label="Extra URL Padding" unit="bytes">
          <input type="number"
            value={profile.path_extra_bytes ?? ''}
            placeholder="none"
            onChange={(e) => setNum('path_extra_bytes', e.target.value)} />
        </Field>
      </Section>

      {/* === TIMING === */}
      <Section title="Timing" defaultOpen={false}>
        <Row>
          <Field label="Connect Timeout" unit="ms">
            <input type="number"
              value={profile.connect_timeout_ms ?? 5000}
              onChange={(e) => setNum('connect_timeout_ms', e.target.value, false)} />
          </Field>
          <Field label="Response Timeout" unit="ms">
            <input type="number"
              value={profile.response_timeout_ms ?? 30000}
              onChange={(e) => setNum('response_timeout_ms', e.target.value, false)} />
          </Field>
        </Row>
      </Section>

      {/* === NETWORK === */}
      <Section title="Network" defaultOpen={false}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <input
            type="checkbox"
            id="use_ns"
            checked={profile.use_namespace ?? false}
            onChange={(e) => set('use_namespace', e.target.checked)}
            style={{ width: 'auto' }}
          />
          <label htmlFor="use_ns" style={{ marginBottom: 0, cursor: 'pointer' }}>
            Use Network Namespace (requires root)
          </label>
        </div>
        {profile.use_namespace && (
          <Field label="NS Prefix">
            <input value={profile.netns_prefix ?? 'nm'} onChange={(e) => set('netns_prefix', e.target.value)} />
          </Field>
        )}
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <input
            type="checkbox"
            id="tcp_quickack"
            checked={profile.tcp_quickack ?? false}
            onChange={(e) => set('tcp_quickack', e.target.checked)}
            style={{ width: 'auto' }}
          />
          <label htmlFor="tcp_quickack" style={{ marginBottom: 0, cursor: 'pointer' }}>
            TCP_QUICKACK (disable delayed ACK)
          </label>
        </div>
        <Row>
          <Field label="Virtual Clients">
            <input type="number" value={profile.num_clients ?? 1}
              onChange={(e) => set('num_clients', Number(e.target.value))} />
          </Field>
          <Field label="Virtual Servers">
            <input type="number" value={profile.num_servers ?? 1}
              onChange={(e) => set('num_servers', Number(e.target.value))} />
          </Field>
        </Row>
      </Section>

      {/* Start / Stop */}
      <div style={{ display: 'flex', gap: 8, marginTop: 4 }}>
        <button
          className="btn-primary"
          onClick={() => startTest(profile)}
          disabled={isRunning}
          style={{ flex: 1 }}
        >
          {isRunning ? 'Running…' : 'Start Test'}
        </button>
        <button
          className="btn-danger"
          onClick={stopTest}
          disabled={!isRunning}
          style={{ flex: 1 }}
        >
          Stop
        </button>
      </div>
    </div>
  )
}
