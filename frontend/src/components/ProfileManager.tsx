import { useState } from 'react'
import { TestProfile } from '../api/client'
import { useTestStore } from '../store/testStore'
import { v4 as uuidv4 } from 'uuid'

export default function ProfileManager() {
  const { savedProfiles, saveProfile, deleteProfile } = useTestStore()
  const [editing, setEditing] = useState<TestProfile | null>(null)

  const handleNew = () => {
    setEditing({
      id: uuidv4(),
      name: 'New Profile',
      test_type: 'cps',
      protocol: 'http1',
      target_host: '127.0.0.1',
      target_port: 8080,
      duration_secs: 60,
      target_cps: 100,
      method: 'GET',
      path: '/',
    })
  }

  const handleSave = async () => {
    if (!editing) return
    await saveProfile(editing)
    setEditing(null)
  }

  const set = <K extends keyof TestProfile>(key: K, value: TestProfile[K]) => {
    setEditing((prev) => prev ? { ...prev, [key]: value } : prev)
  }

  return (
    <div style={{ display: 'flex', gap: 20 }}>
      {/* 프로파일 목록 */}
      <div style={{ flex: 1 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, fontWeight: 600 }}>Saved Profiles</h2>
          <button className="btn-secondary" onClick={handleNew}>+ New</button>
        </div>

        {savedProfiles.length === 0 ? (
          <div className="card" style={{ color: '#8b949e', fontSize: 14, textAlign: 'center', padding: 32 }}>
            No saved profiles. Click "+ New" to create one.
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {savedProfiles.map((p) => (
              <div key={p.id} className="card" style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                <div style={{ flex: 1 }}>
                  <div style={{ fontWeight: 600, fontSize: 14 }}>{p.name}</div>
                  <div style={{ fontSize: 12, color: '#8b949e' }}>
                    {p.test_type.toUpperCase()} / {p.protocol.toUpperCase()} &mdash;{' '}
                    {p.target_host}:{p.target_port} &mdash; {p.duration_secs}s
                  </div>
                </div>
                <button className="btn-secondary" onClick={() => setEditing({ ...p })}>Edit</button>
                <button className="btn-danger" onClick={() => deleteProfile(p.id)}>Delete</button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* 편집 패널 */}
      {editing && (
        <div className="card" style={{ width: 340, flexShrink: 0, display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div className="card-title">{editing.id ? 'Edit Profile' : 'New Profile'}</div>

          <div>
            <label>Name</label>
            <input value={editing.name} onChange={(e) => set('name', e.target.value)} />
          </div>
          <div>
            <label>Test Type</label>
            <select value={editing.test_type} onChange={(e) => set('test_type', e.target.value as TestProfile['test_type'])}>
              <option value="cps">CPS</option>
              <option value="bw">BW</option>
              <option value="cc">CC</option>
            </select>
          </div>
          <div>
            <label>Protocol</label>
            <select value={editing.protocol} onChange={(e) => set('protocol', e.target.value as TestProfile['protocol'])}>
              <option value="http1">HTTP/1.1</option>
              <option value="http2">HTTP/2</option>
            </select>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 80px', gap: 8 }}>
            <div>
              <label>Host</label>
              <input value={editing.target_host} onChange={(e) => set('target_host', e.target.value)} />
            </div>
            <div>
              <label>Port</label>
              <input type="number" value={editing.target_port} onChange={(e) => set('target_port', Number(e.target.value))} />
            </div>
          </div>
          <div>
            <label>Duration (s)</label>
            <input type="number" value={editing.duration_secs} onChange={(e) => set('duration_secs', Number(e.target.value))} />
          </div>
          {editing.test_type === 'cps' && (
            <div>
              <label>Target CPS</label>
              <input type="number" value={editing.target_cps ?? 100} onChange={(e) => set('target_cps', Number(e.target.value))} />
            </div>
          )}
          {(editing.test_type === 'cc' || editing.test_type === 'bw') && (
            <div>
              <label>Target CC</label>
              <input type="number" value={editing.target_cc ?? 50} onChange={(e) => set('target_cc', Number(e.target.value))} />
            </div>
          )}
          <div style={{ display: 'flex', gap: 8 }}>
            <button className="btn-primary" onClick={handleSave} style={{ flex: 1 }}>Save</button>
            <button className="btn-secondary" onClick={() => setEditing(null)} style={{ flex: 1 }}>Cancel</button>
          </div>
        </div>
      )}
    </div>
  )
}
