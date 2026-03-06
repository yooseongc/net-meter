import { useState } from 'react'
import { TestProfile, TestType, Protocol } from '../api/client'
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
})

export default function TestControl() {
  const { testState, startTest, stopTest, savedProfiles } = useTestStore()
  const [profile, setProfile] = useState<TestProfile>(defaultProfile)

  const isRunning = testState === 'running' || testState === 'preparing' || testState === 'stopping'

  const handleStart = async () => {
    await startTest(profile)
  }

  const handleStop = async () => {
    await stopTest()
  }

  const loadProfile = (id: string) => {
    const p = savedProfiles.find((x) => x.id === id)
    if (p) setProfile({ ...p })
  }

  const set = <K extends keyof TestProfile>(key: K, value: TestProfile[K]) => {
    setProfile((prev) => ({ ...prev, [key]: value }))
  }

  return (
    <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
      <div className="card-title">Test Control</div>

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

      <div>
        <label>Profile Name</label>
        <input value={profile.name} onChange={(e) => set('name', e.target.value)} />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
        <div>
          <label>Test Type</label>
          <select
            value={profile.test_type}
            onChange={(e) => set('test_type', e.target.value as TestType)}
          >
            <option value="cps">CPS</option>
            <option value="bw">BW</option>
            <option value="cc">CC</option>
          </select>
        </div>
        <div>
          <label>Protocol</label>
          <select
            value={profile.protocol}
            onChange={(e) => set('protocol', e.target.value as Protocol)}
          >
            <option value="http1">HTTP/1.1</option>
            <option value="http2">HTTP/2</option>
          </select>
        </div>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr auto', gap: 10 }}>
        <div>
          <label>Target Host</label>
          <input value={profile.target_host} onChange={(e) => set('target_host', e.target.value)} />
        </div>
        <div>
          <label>Port</label>
          <input
            type="number"
            value={profile.target_port}
            onChange={(e) => set('target_port', Number(e.target.value))}
            style={{ width: 80 }}
          />
        </div>
      </div>

      <div>
        <label>Duration (seconds, 0 = manual stop)</label>
        <input
          type="number"
          value={profile.duration_secs}
          onChange={(e) => set('duration_secs', Number(e.target.value))}
        />
      </div>

      {profile.test_type === 'cps' && (
        <div>
          <label>Target CPS</label>
          <input
            type="number"
            value={profile.target_cps ?? 100}
            onChange={(e) => set('target_cps', Number(e.target.value))}
          />
        </div>
      )}

      {(profile.test_type === 'cc' || profile.test_type === 'bw') && (
        <div>
          <label>Target Concurrent Connections</label>
          <input
            type="number"
            value={profile.target_cc ?? 50}
            onChange={(e) => set('target_cc', Number(e.target.value))}
          />
        </div>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
        <div>
          <label>HTTP Method</label>
          <select
            value={profile.method}
            onChange={(e) => set('method', e.target.value as 'GET' | 'POST')}
          >
            <option value="GET">GET</option>
            <option value="POST">POST</option>
          </select>
        </div>
        <div>
          <label>Path</label>
          <input value={profile.path} onChange={(e) => set('path', e.target.value)} />
        </div>
      </div>

      <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
        <button
          className="btn-primary"
          onClick={handleStart}
          disabled={isRunning}
          style={{ flex: 1 }}
        >
          {isRunning ? 'Running…' : 'Start Test'}
        </button>
        <button
          className="btn-danger"
          onClick={handleStop}
          disabled={!isRunning}
          style={{ flex: 1 }}
        >
          Stop
        </button>
      </div>
    </div>
  )
}
