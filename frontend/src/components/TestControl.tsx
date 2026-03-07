import { useState, useEffect } from 'react'
import {
  TestConfig, TestType, Protocol, HttpMethod,
  PairConfig, PayloadProfile, LoadConfig, NsConfig,
  TcpPayload, HttpPayload, Thresholds,
} from '../api/client'
import { useTestStore } from '../store/testStore'
import { v4 as uuidv4 } from 'uuid'

// ---------------------------------------------------------------------------
// 기본값
// ---------------------------------------------------------------------------

const defaultHttpPayload = (): HttpPayload => ({
  type: 'http', method: 'GET', path: '/',
})

const defaultTcpPayload = (): TcpPayload => ({
  type: 'tcp', tx_bytes: 64, rx_bytes: 64,
})

const defaultPayloadForProtocol = (proto: Protocol): PayloadProfile =>
  proto === 'tcp' ? defaultTcpPayload() : defaultHttpPayload()

const defaultPair = (idx: number): PairConfig => ({
  id: uuidv4(),
  client: { id: `client-${idx}` },
  server: { id: `server-${idx}`, port: 8080 },
  protocol: 'http1',
  payload: defaultHttpPayload(),
})

const defaultConfig = (): TestConfig => ({
  id: uuidv4(),
  name: 'New Test',
  test_type: 'cps',
  duration_secs: 30,
  default_load: { target_cps: 100, connect_timeout_ms: 5000, response_timeout_ms: 30000, ramp_up_secs: 0 },
  pairs: [defaultPair(0)],
  ns_config: { use_namespace: false, netns_prefix: 'nm', tcp_quickack: false },
  thresholds: {},
})

// ---------------------------------------------------------------------------
// 소형 UI 컴포넌트
// ---------------------------------------------------------------------------

function Section({ title, children, defaultOpen = true }: {
  title: string; children: React.ReactNode; defaultOpen?: boolean
}) {
  const [open, setOpen] = useState(defaultOpen)
  return (
    <div style={{ borderTop: '1px solid #21262d', paddingTop: 10 }}>
      <button
        onClick={() => setOpen(!open)}
        style={{ background: 'none', color: '#8b949e', fontSize: 11, fontWeight: 700, textTransform: 'uppercase', letterSpacing: '0.08em', padding: '0 0 6px 0', display: 'flex', alignItems: 'center', gap: 6, width: '100%', textAlign: 'left' }}
      >
        <span style={{ fontSize: 10, color: '#58a6ff' }}>{open ? '▼' : '▶'}</span>
        {title}
      </button>
      {open && <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>{children}</div>}
    </div>
  )
}

function Row({ children }: { children: React.ReactNode }) {
  return <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>{children}</div>
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

// ---------------------------------------------------------------------------
// Pair 편집 다이얼로그
// ---------------------------------------------------------------------------

function PairDialog({
  pair,
  onSave,
  onCancel,
}: {
  pair: PairConfig
  onSave: (p: PairConfig) => void
  onCancel: () => void
}) {
  const [p, setP] = useState<PairConfig>({ ...pair })

  const setProtocol = (proto: Protocol) => {
    setP((prev) => ({
      ...prev,
      protocol: proto,
      payload: defaultPayloadForProtocol(proto),
    }))
  }

  const setPayloadField = (key: string, val: unknown) =>
    setP((prev) => ({ ...prev, payload: { ...prev.payload, [key]: val } as PayloadProfile }))

  const setClientField = (key: keyof PairConfig['client'], val: string) =>
    setP((prev) => ({ ...prev, client: { ...prev.client, [key]: val || undefined } }))

  const setServerField = (key: keyof PairConfig['server'], val: string | number) =>
    setP((prev) => ({ ...prev, server: { ...prev.server, [key]: val === '' ? undefined : val } }))

  const [useLoadOverride, setUseLoadOverride] = useState(!!p.load)
  const [loadOverride, setLoadOverride] = useState<LoadConfig>(p.load ?? {})

  const setLoadField = (key: keyof LoadConfig, raw: string) => {
    const n = raw === '' ? undefined : Number(raw)
    setLoadOverride((prev) => ({ ...prev, [key]: n }))
  }

  const handleSave = () => {
    onSave({ ...p, load: useLoadOverride ? loadOverride : undefined })
  }

  const isTcp = p.protocol === 'tcp'
  const payload = p.payload as (TcpPayload & { type?: string }) | (HttpPayload & { type?: string })

  return (
    <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.6)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000 }}>
      <div className="card" style={{ width: 500, maxHeight: '90vh', overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div className="card-title" style={{ margin: 0 }}>Edit Pair</div>

        {/* Protocol */}
        <Field label="Protocol">
          <select value={p.protocol} onChange={(e) => setProtocol(e.target.value as Protocol)}>
            <option value="tcp">TCP</option>
            <option value="http1">HTTP/1.1</option>
            <option value="http2">HTTP/2</option>
          </select>
        </Field>

        {/* Endpoints */}
        <Row>
          <div>
            <div style={{ fontSize: 11, fontWeight: 700, color: '#58a6ff', marginBottom: 6, textTransform: 'uppercase' }}>Client</div>
            <Field label="ID">
              <input value={p.client.id} onChange={(e) => setClientField('id', e.target.value)} />
            </Field>
            <Field label="IP (NS mode)" unit="optional">
              <input value={p.client.ip ?? ''} placeholder="auto" onChange={(e) => setClientField('ip', e.target.value)} />
            </Field>
            <Field label="Workers" unit="병렬 클라이언트 수">
              <input type="number" min={1} max={64} value={p.client_count ?? 1}
                onChange={(e) => setP((prev) => ({ ...prev, client_count: Math.max(1, Number(e.target.value)) }))} />
            </Field>
          </div>
          <div>
            <div style={{ fontSize: 11, fontWeight: 700, color: '#3fb950', marginBottom: 6, textTransform: 'uppercase' }}>Server</div>
            <Field label="ID">
              <input value={p.server.id} onChange={(e) => setServerField('id', e.target.value)} />
            </Field>
            <Row>
              <Field label="IP" unit="optional">
                <input value={p.server.ip ?? ''} placeholder="0.0.0.0" onChange={(e) => setServerField('ip', e.target.value)} />
              </Field>
              <Field label="Port">
                <input type="number" value={p.server.port} onChange={(e) => setServerField('port', Number(e.target.value))} />
              </Field>
            </Row>
          </div>
        </Row>

        {/* TLS (HTTP only) */}
        {!isTcp && (
          <Field label="TLS">
            <label style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <input type="checkbox" checked={p.tls ?? false}
                onChange={(e) => setP((prev) => ({ ...prev, tls: e.target.checked }))}
                style={{ width: 'auto' }} />
              Enable TLS (self-signed cert, HTTP/1.1 HTTPS / HTTP/2 over TLS)
            </label>
          </Field>
        )}

        {/* Payload */}
        {isTcp ? (
          <>
            <div style={{ fontSize: 11, fontWeight: 700, color: '#8b949e', textTransform: 'uppercase' }}>TCP Payload</div>
            <Row>
              <Field label="TX bytes" unit="client→server">
                <input type="number" value={(payload as TcpPayload).tx_bytes ?? 0}
                  onChange={(e) => setPayloadField('tx_bytes', Number(e.target.value))} />
              </Field>
              <Field label="RX bytes" unit="server→client">
                <input type="number" value={(payload as TcpPayload).rx_bytes ?? 0}
                  onChange={(e) => setPayloadField('rx_bytes', Number(e.target.value))} />
              </Field>
            </Row>
          </>
        ) : (
          <>
            <div style={{ fontSize: 11, fontWeight: 700, color: '#8b949e', textTransform: 'uppercase' }}>HTTP Payload</div>
            <Row>
              <Field label="Method">
                <select value={(payload as HttpPayload).method ?? 'GET'}
                  onChange={(e) => setPayloadField('method', e.target.value as HttpMethod)}>
                  <option value="GET">GET</option>
                  <option value="POST">POST</option>
                </select>
              </Field>
              <Field label="Path">
                <input value={(payload as HttpPayload).path ?? '/'}
                  onChange={(e) => setPayloadField('path', e.target.value)} />
              </Field>
            </Row>
            <Row>
              <Field label="Request Body" unit="bytes">
                <input type="number" value={(payload as HttpPayload).request_body_bytes ?? ''}
                  placeholder="none"
                  onChange={(e) => setPayloadField('request_body_bytes', e.target.value === '' ? undefined : Number(e.target.value))} />
              </Field>
              <Field label="Response Body" unit="bytes">
                <input type="number" value={(payload as HttpPayload).response_body_bytes ?? ''}
                  placeholder="none"
                  onChange={(e) => setPayloadField('response_body_bytes', e.target.value === '' ? undefined : Number(e.target.value))} />
              </Field>
            </Row>
            <Row>
              <Field label="URL Padding" unit="bytes">
                <input type="number" value={(payload as HttpPayload).path_extra_bytes ?? ''}
                  placeholder="none"
                  onChange={(e) => setPayloadField('path_extra_bytes', e.target.value === '' ? undefined : Number(e.target.value))} />
              </Field>
              {p.protocol === 'http2' && (
                <Field label="Max Streams" unit="BW mode">
                  <input type="number" value={(payload as HttpPayload).h2_max_concurrent_streams ?? 10}
                    onChange={(e) => setPayloadField('h2_max_concurrent_streams', Number(e.target.value))} />
                </Field>
              )}
            </Row>
          </>
        )}

        {/* Load override */}
        <div style={{ borderTop: '1px solid #21262d', paddingTop: 8 }}>
          <label style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <input type="checkbox" checked={useLoadOverride} onChange={(e) => setUseLoadOverride(e.target.checked)} style={{ width: 'auto' }} />
            Override load settings for this pair
          </label>
          {useLoadOverride && (
            <Row>
              <Field label="Target CPS" unit="/s">
                <input type="number" value={loadOverride.target_cps ?? ''} placeholder="default"
                  onChange={(e) => setLoadField('target_cps', e.target.value)} />
              </Field>
              <Field label="Target CC">
                <input type="number" value={loadOverride.target_cc ?? ''} placeholder="default"
                  onChange={(e) => setLoadField('target_cc', e.target.value)} />
              </Field>
            </Row>
          )}
        </div>

        {/* Actions */}
        <div style={{ display: 'flex', gap: 8 }}>
          <button className="btn-primary" onClick={handleSave} style={{ flex: 1 }}>Save Pair</button>
          <button className="btn-secondary" onClick={onCancel} style={{ flex: 1 }}>Cancel</button>
        </div>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// 메인 컴포넌트
// ---------------------------------------------------------------------------

export default function TestControl() {
  const { testState, startTest, stopTest, savedProfiles, saveProfile, draftConfig, setDraftConfig } = useTestStore()
  const [config, setConfig] = useState<TestConfig>(defaultConfig)
  const [editingPair, setEditingPair] = useState<PairConfig | null>(null)
  const [isNewPair, setIsNewPair] = useState(false)
  const [saveMsg, setSaveMsg] = useState<string | null>(null)

  const isRunning = testState === 'running' || testState === 'preparing' || testState === 'stopping'

  // Profiles 탭에서 "Load" 클릭 시 draftConfig가 설정되면 적용
  useEffect(() => {
    if (draftConfig) {
      setConfig({ ...draftConfig })
      setDraftConfig(null)
    }
  }, [draftConfig])

  const setField = <K extends keyof TestConfig>(key: K, val: TestConfig[K]) =>
    setConfig((prev) => ({ ...prev, [key]: val }))

  const setLoadField = (key: keyof LoadConfig, raw: string) => {
    const n = raw === '' ? undefined : Number(raw)
    setConfig((prev) => ({ ...prev, default_load: { ...prev.default_load, [key]: n } }))
  }

  const setNsField = <K extends keyof NsConfig>(key: K, val: NsConfig[K]) =>
    setConfig((prev) => ({ ...prev, ns_config: { ...prev.ns_config, [key]: val } }))

  const setThresholdField = (key: keyof Thresholds, raw: string | boolean) => {
    const val = typeof raw === 'boolean' ? raw : (raw === '' ? undefined : Number(raw))
    setConfig((prev) => ({ ...prev, thresholds: { ...prev.thresholds, [key]: val } }))
  }

  // Pairs
  const handleAddPair = () => {
    const newPair = defaultPair(config.pairs.length)
    setIsNewPair(true)
    setEditingPair(newPair)
  }

  const handleEditPair = (pair: PairConfig) => {
    setIsNewPair(false)
    setEditingPair({ ...pair })
  }

  const handleDeletePair = (id: string) => {
    setConfig((prev) => ({ ...prev, pairs: prev.pairs.filter((p) => p.id !== id) }))
  }

  const handleSavePair = (saved: PairConfig) => {
    setConfig((prev) => {
      if (isNewPair) return { ...prev, pairs: [...prev.pairs, saved] }
      return { ...prev, pairs: prev.pairs.map((p) => (p.id === saved.id ? saved : p)) }
    })
    setEditingPair(null)
  }

  const loadProfile = (id: string) => {
    const p = savedProfiles.find((x) => x.id === id)
    if (p) setConfig({ ...p })
  }

  const exportConfig = () => {
    const blob = new Blob([JSON.stringify(config, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `${config.name.replace(/\s+/g, '_')}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  const importConfig = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = (ev) => {
      try {
        const data = JSON.parse(ev.target?.result as string) as TestConfig
        setConfig({ ...data, id: uuidv4() })
      } catch { /* ignore */ }
    }
    reader.readAsText(file)
    e.target.value = ''
  }

  const protoLabel = (p: Protocol) => p === 'tcp' ? 'TCP' : p === 'http1' ? 'HTTP/1.1' : 'HTTP/2'

  return (
    <>
      {/* Pair 편집 다이얼로그 */}
      {editingPair && (
        <PairDialog
          pair={editingPair}
          onSave={handleSavePair}
          onCancel={() => setEditingPair(null)}
        />
      )}

      <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        {/* 헤더 */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div className="card-title" style={{ margin: 0 }}>Test Config</div>
          <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
            {saveMsg && <span style={{ fontSize: 11, color: '#3fb950' }}>{saveMsg}</span>}
            <button className="btn-secondary"
              onClick={async () => {
                await saveProfile(config)
                setSaveMsg('Saved!')
                setTimeout(() => setSaveMsg(null), 2000)
              }}
              style={{ padding: '4px 10px', fontSize: 11 }}>
              Save to Profiles
            </button>
            <button className="btn-secondary" onClick={exportConfig} style={{ padding: '4px 10px', fontSize: 11 }}>Export</button>
            <label className="btn-secondary" style={{ padding: '4px 10px', fontSize: 11, cursor: 'pointer', borderRadius: 6, border: '1px solid #30363d', background: '#21262d', color: '#e6edf3', fontWeight: 600 }}>
              Import
              <input type="file" accept=".json" onChange={importConfig} style={{ display: 'none' }} />
            </label>
          </div>
        </div>

        {/* 저장된 설정 불러오기 */}
        {savedProfiles.length > 0 && (
          <div>
            <label>Load Saved Config</label>
            <select onChange={(e) => loadProfile(e.target.value)} defaultValue="">
              <option value="" disabled>Select…</option>
              {savedProfiles.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </div>
        )}

        {/* BASIC */}
        <Section title="Basic" defaultOpen>
          <Field label="Config Name">
            <input value={config.name} onChange={(e) => setField('name', e.target.value)} />
          </Field>
          <Row>
            <Field label="Test Type">
              <select value={config.test_type} onChange={(e) => setField('test_type', e.target.value as TestType)}>
                <option value="cps">CPS — Connections/s</option>
                <option value="cc">CC — Concurrent Connections</option>
                <option value="bw">BW — Bandwidth</option>
              </select>
            </Field>
            <Field label="Duration" unit="sec (0=manual)">
              <input type="number" value={config.duration_secs}
                onChange={(e) => setField('duration_secs', Number(e.target.value))} />
            </Field>
          </Row>
        </Section>

        {/* DEFAULT LOAD */}
        <Section title="Default Load" defaultOpen>
          {config.test_type === 'cps' ? (
            <Field label="Target CPS" unit="/s">
              <input type="number" value={config.default_load.target_cps ?? 100}
                onChange={(e) => setLoadField('target_cps', e.target.value)} />
            </Field>
          ) : (
            <Field label="Target Concurrent Connections">
              <input type="number" value={config.default_load.target_cc ?? 50}
                onChange={(e) => setLoadField('target_cc', e.target.value)} />
            </Field>
          )}
          <Row>
            <Field label="Connect Timeout" unit="ms">
              <input type="number" value={config.default_load.connect_timeout_ms ?? 5000}
                onChange={(e) => setLoadField('connect_timeout_ms', e.target.value)} />
            </Field>
            <Field label="Response Timeout" unit="ms">
              <input type="number" value={config.default_load.response_timeout_ms ?? 30000}
                onChange={(e) => setLoadField('response_timeout_ms', e.target.value)} />
            </Field>
          </Row>
          <Field label="Max In-flight" unit="blank=auto">
            <input type="number" value={config.default_load.max_inflight ?? ''}
              placeholder="auto"
              onChange={(e) => setLoadField('max_inflight', e.target.value)} />
          </Field>
          <Field label="Ramp-up" unit="sec (0=off)">
            <input type="number" value={config.default_load.ramp_up_secs ?? 0}
              min={0}
              onChange={(e) => setLoadField('ramp_up_secs', e.target.value)} />
          </Field>
        </Section>

        {/* THRESHOLDS */}
        <Section title="Thresholds / Alarm" defaultOpen={false}>
          <div style={{ fontSize: 11, color: '#8b949e', marginBottom: 4 }}>
            임계값 초과 시 대시보드에 경고가 표시됩니다. auto_stop 활성화 시 시험을 자동 중단합니다.
          </div>
          <Row>
            <Field label="Min CPS" unit="/s (blank=off)">
              <input type="number" value={config.thresholds?.min_cps ?? ''}
                placeholder="none"
                onChange={(e) => setThresholdField('min_cps', e.target.value)} />
            </Field>
            <Field label="Max Error Rate" unit="% (blank=off)">
              <input type="number" value={config.thresholds?.max_error_rate_pct ?? ''}
                placeholder="none"
                onChange={(e) => setThresholdField('max_error_rate_pct', e.target.value)} />
            </Field>
          </Row>
          <Field label="Max Latency p99" unit="ms (blank=off)">
            <input type="number" value={config.thresholds?.max_latency_p99_ms ?? ''}
              placeholder="none"
              onChange={(e) => setThresholdField('max_latency_p99_ms', e.target.value)} />
          </Field>
          <label style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <input type="checkbox"
              checked={config.thresholds?.auto_stop_on_fail ?? false}
              onChange={(e) => setThresholdField('auto_stop_on_fail', e.target.checked)}
              style={{ width: 'auto' }} />
            Auto-stop on violation
          </label>
        </Section>

        {/* NS CONFIG */}
        <Section title="Network" defaultOpen={false}>
          <label style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <input type="checkbox" checked={config.ns_config.use_namespace}
              onChange={(e) => setNsField('use_namespace', e.target.checked)}
              style={{ width: 'auto' }} />
            Use Network Namespace (requires root / CAP_NET_ADMIN)
          </label>
          {config.ns_config.use_namespace && (
            <Field label="NS Prefix">
              <input value={config.ns_config.netns_prefix}
                onChange={(e) => setNsField('netns_prefix', e.target.value)} />
            </Field>
          )}
          <label style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <input type="checkbox" checked={config.ns_config.tcp_quickack}
              onChange={(e) => setNsField('tcp_quickack', e.target.checked)}
              style={{ width: 'auto' }} />
            TCP_QUICKACK (disable delayed ACK)
          </label>
        </Section>

        {/* PAIRS */}
        <Section title={`Pairs (${config.pairs.length})`} defaultOpen>
          <div style={{ overflowX: 'auto' }}>
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
              <thead>
                <tr style={{ color: '#8b949e', textAlign: 'left' }}>
                  <th style={{ padding: '4px 6px' }}>Protocol</th>
                  <th style={{ padding: '4px 6px' }}>Client</th>
                  <th style={{ padding: '4px 6px' }}>Server</th>
                  <th style={{ padding: '4px 6px' }}>Load</th>
                  <th style={{ padding: '4px 6px', width: 90 }}></th>
                </tr>
              </thead>
              <tbody>
                {config.pairs.map((pair) => (
                  <tr key={pair.id} style={{ borderTop: '1px solid #21262d' }}>
                    <td style={{ padding: '5px 6px', fontWeight: 600, color: '#58a6ff' }}>
                      {protoLabel(pair.protocol)}
                    </td>
                    <td style={{ padding: '5px 6px', color: '#8b949e' }}>
                      {pair.client.id}
                      {pair.client.ip && <span style={{ color: '#484f58' }}> ({pair.client.ip})</span>}
                    </td>
                    <td style={{ padding: '5px 6px', color: '#8b949e' }}>
                      {pair.server.id} — {pair.server.ip ?? '0.0.0.0'}:{pair.server.port}
                    </td>
                    <td style={{ padding: '5px 6px', color: '#484f58' }}>
                      {pair.load ? 'custom' : 'default'}
                    </td>
                    <td style={{ padding: '5px 6px' }}>
                      <div style={{ display: 'flex', gap: 4 }}>
                        <button className="btn-secondary" onClick={() => handleEditPair(pair)}
                          style={{ padding: '2px 8px', fontSize: 11 }}>Edit</button>
                        <button className="btn-danger" onClick={() => handleDeletePair(pair.id)}
                          disabled={config.pairs.length <= 1}
                          style={{ padding: '2px 8px', fontSize: 11 }}>✕</button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <button className="btn-secondary" onClick={handleAddPair}
            style={{ alignSelf: 'flex-start', padding: '4px 12px', fontSize: 12 }}>
            + Add Pair
          </button>
        </Section>

        {/* Start / Stop */}
        <div style={{ display: 'flex', gap: 8, marginTop: 4 }}>
          <button className="btn-primary" onClick={() => startTest(config)}
            disabled={isRunning || config.pairs.length === 0} style={{ flex: 1 }}>
            {isRunning ? 'Running…' : 'Start Test'}
          </button>
          <button className="btn-danger" onClick={stopTest} disabled={!isRunning} style={{ flex: 1 }}>
            Stop
          </button>
        </div>
      </div>
    </>
  )
}
