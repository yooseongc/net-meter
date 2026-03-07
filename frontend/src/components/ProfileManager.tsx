import { useState } from 'react'
import { TestConfig } from '../api/client'
import { useTestStore } from '../store/testStore'

function configSummary(c: TestConfig): string {
  const protos = [...new Set(c.associations.map((a) => a.protocol.toUpperCase()))].join('/')
  return `${c.test_type.toUpperCase()} · ${protos} · ${c.associations.length} association(s) · ${c.duration_secs}s`
}

export default function ProfileManager({ onLoadConfig }: { onLoadConfig?: (c: TestConfig) => void }) {
  const { savedProfiles, deleteProfile } = useTestStore()
  const [expanded, setExpanded] = useState<string | null>(null)

  return (
    <div style={{ maxWidth: 800 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, fontWeight: 600 }}>Saved Configs</h2>
        <span style={{ fontSize: 12, color: '#8b949e' }}>
          Config 탭에서 "Save to Profiles"로 저장합니다.
        </span>
      </div>

      {savedProfiles.length === 0 ? (
        <div className="card" style={{ color: '#8b949e', fontSize: 14, textAlign: 'center', padding: 32 }}>
          저장된 설정이 없습니다. Config 탭에서 "Save to Profiles"를 눌러 저장하세요.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {savedProfiles.map((c) => (
            <div key={c.id} className="card" style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                <div style={{ flex: 1 }}>
                  <div style={{ fontWeight: 600, fontSize: 14 }}>{c.name}</div>
                  <div style={{ fontSize: 12, color: '#8b949e' }}>{configSummary(c)}</div>
                </div>
                <button className="btn-secondary"
                  onClick={() => setExpanded(expanded === c.id ? null : c.id)}
                  style={{ padding: '3px 10px', fontSize: 12 }}>
                  {expanded === c.id ? 'Hide' : 'Details'}
                </button>
                {onLoadConfig && (
                  <button className="btn-primary"
                    onClick={() => onLoadConfig(c)}
                    style={{ padding: '3px 10px', fontSize: 12 }}>
                    Load
                  </button>
                )}
                <button className="btn-danger"
                  onClick={() => deleteProfile(c.id)}
                  style={{ padding: '3px 10px', fontSize: 12 }}>
                  Delete
                </button>
              </div>

              {expanded === c.id && (
                <div style={{ borderTop: '1px solid #21262d', paddingTop: 8 }}>
                  {/* Associations table */}
                  <div style={{ fontSize: 11, color: '#8b949e', marginBottom: 6 }}>ASSOCIATIONS</div>
                  <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
                    <thead>
                      <tr style={{ color: '#8b949e' }}>
                        <th style={{ textAlign: 'left', padding: '2px 6px' }}>Protocol</th>
                        <th style={{ textAlign: 'left', padding: '2px 6px' }}>Client</th>
                        <th style={{ textAlign: 'left', padding: '2px 6px' }}>Server</th>
                        <th style={{ textAlign: 'left', padding: '2px 6px' }}>Load</th>
                      </tr>
                    </thead>
                    <tbody>
                      {c.associations.map((assoc) => (
                        <tr key={assoc.id} style={{ borderTop: '1px solid #21262d' }}>
                          <td style={{ padding: '4px 6px', color: '#58a6ff', fontWeight: 600 }}>
                            {assoc.protocol.toUpperCase()}
                          </td>
                          <td style={{ padding: '4px 6px', color: '#8b949e' }}>
                            {assoc.client_net.base_ip}/{assoc.client_net.prefix_len ?? 24}
                          </td>
                          <td style={{ padding: '4px 6px', color: '#8b949e' }}>
                            {assoc.server.ip ?? '0.0.0.0'}:{assoc.server.port}
                          </td>
                          <td style={{ padding: '4px 6px', color: '#484f58' }}>
                            {assoc.load ? 'custom' : 'default'}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>

                  {/* JSON 보기 */}
                  <details style={{ marginTop: 8 }}>
                    <summary style={{ fontSize: 11, color: '#8b949e', cursor: 'pointer' }}>Raw JSON</summary>
                    <pre style={{ fontSize: 10, color: '#8b949e', background: '#0d1117', padding: 8, borderRadius: 4, overflow: 'auto', maxHeight: 200, marginTop: 4 }}>
                      {JSON.stringify(c, null, 2)}
                    </pre>
                  </details>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
