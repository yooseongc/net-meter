import { useState, useEffect } from 'react'
import {
  TestConfig, TestType, Protocol, HttpMethod,
  Association, PayloadProfile, LoadConfig, NetworkConfig, NetworkMode,
  TcpPayload, HttpPayload, Thresholds, VlanConfig,
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

const defaultAssociation = (idx: number): Association => ({
  id: uuidv4(),
  name: `assoc-${idx}`,
  client_net: { base_ip: `10.10.1.${idx + 1}`, prefix_len: 24 },
  server: { id: `server-${idx}`, port: 8080 },
  protocol: 'http1',
  payload: defaultHttpPayload(),
})

const defaultNetworkConfig = (): NetworkConfig => ({
  mode: 'loopback',
  ns: { netns_prefix: 'nm' },
  tcp_quickack: false,
})

const defaultConfig = (): TestConfig => ({
  id: uuidv4(),
  name: 'New Test',
  test_type: 'cps',
  duration_secs: 30,
  total_clients: 0,
  default_load: { cps_per_client: 100, connect_timeout_ms: 5000, response_timeout_ms: 30000, ramp_up_secs: 0 },
  associations: [defaultAssociation(0)],
  network: defaultNetworkConfig(),
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
// Association 편집 다이얼로그
// ---------------------------------------------------------------------------

function AssociationDialog({
  assoc,
  onSave,
  onCancel,
}: {
  assoc: Association
  onSave: (a: Association) => void
  onCancel: () => void
}) {
  const [a, setA] = useState<Association>({ ...assoc })

  const setProtocol = (proto: Protocol) => {
    setA((prev) => ({
      ...prev,
      protocol: proto,
      payload: defaultPayloadForProtocol(proto),
    }))
  }

  const setPayloadField = (key: string, val: unknown) =>
    setA((prev) => ({ ...prev, payload: { ...prev.payload, [key]: val } as PayloadProfile }))

  const setClientNetField = (key: keyof Association['client_net'], val: string | number | undefined) =>
    setA((prev) => ({ ...prev, client_net: { ...prev.client_net, [key]: val } }))

  const setServerField = (key: keyof Association['server'], val: string | number) =>
    setA((prev) => ({ ...prev, server: { ...prev.server, [key]: val === '' ? undefined : val } }))

  const [useLoadOverride, setUseLoadOverride] = useState(!!a.load)
  const [loadOverride, setLoadOverride] = useState<LoadConfig>(a.load ?? {})
  const setLoadField = (key: keyof LoadConfig, raw: string) => {
    const n = raw === '' ? undefined : Number(raw)
    setLoadOverride((prev) => ({ ...prev, [key]: n }))
  }

  const [useVlan, setUseVlan] = useState(!!a.vlan)
  const [vlan, setVlan] = useState<VlanConfig>(a.vlan ?? { outer_vid: 100 })
  const setVlanField = (key: keyof VlanConfig, val: unknown) =>
    setVlan((prev) => ({ ...prev, [key]: val }))

  const handleSave = () => {
    onSave({
      ...a,
      load: useLoadOverride ? loadOverride : undefined,
      vlan: useVlan ? vlan : undefined,
    })
  }

  const isTcp = a.protocol === 'tcp'
  const payload = a.payload as (TcpPayload & { type?: string }) | (HttpPayload & { type?: string })

  return (
    <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.6)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000 }}>
      <div className="card" style={{ width: 540, maxHeight: '92vh', overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div className="card-title" style={{ margin: 0 }}>Edit Association</div>

        {/* Name & Protocol */}
        <Row>
          <Field label="Name">
            <input value={a.name} onChange={(e) => setA((prev) => ({ ...prev, name: e.target.value }))} />
          </Field>
          <Field label="Protocol">
            <select value={a.protocol} onChange={(e) => setProtocol(e.target.value as Protocol)}>
              <option value="tcp">TCP</option>
              <option value="http1">HTTP/1.1</option>
              <option value="http2">HTTP/2</option>
            </select>
          </Field>
        </Row>

        {/* Client Net */}
        <div>
          <div style={{ fontSize: 11, fontWeight: 700, color: '#58a6ff', marginBottom: 6, textTransform: 'uppercase' }}>Client IP Range</div>
          <Row>
            <Field label="Base IP">
              <input
                value={a.client_net.base_ip}
                placeholder="10.10.1.1"
                onChange={(e) => setClientNetField('base_ip', e.target.value)}
              />
            </Field>
            <Field label="Count (workers)" unit="blank=auto">
              <input
                type="number" min={1}
                value={a.client_net.count ?? ''}
                placeholder="auto"
                onChange={(e) => setClientNetField('count', e.target.value === '' ? undefined : Math.max(1, Number(e.target.value)))}
              />
            </Field>
          </Row>
          <div style={{ marginTop: 6 }}>
            <Field label="Prefix Length" unit="/24">
              <input
                type="number" min={8} max={32}
                value={a.client_net.prefix_len ?? 24}
                onChange={(e) => setClientNetField('prefix_len', Number(e.target.value))}
              />
            </Field>
          </div>
          <div style={{ fontSize: 11, color: '#484f58', marginTop: 4 }}>
            {a.client_net.count
              ? `${a.client_net.count}개 워커: ${a.client_net.base_ip} ~ (base_ip+${a.client_net.count - 1})`
              : 'Count: total_clients / associations.len() 자동 계산 (0이면 1)'}
          </div>
        </div>

        {/* Server */}
        <div>
          <div style={{ fontSize: 11, fontWeight: 700, color: '#3fb950', marginBottom: 6, textTransform: 'uppercase' }}>Server Endpoint</div>
          <Row>
            <Field label="Server ID">
              <input value={a.server.id} onChange={(e) => setServerField('id', e.target.value)} />
            </Field>
            <Field label="Port">
              <input type="number" value={a.server.port} onChange={(e) => setServerField('port', Number(e.target.value))} />
            </Field>
          </Row>
          <div style={{ marginTop: 6 }}>
            <Field label="Server IP" unit="blank=auto (NS: 10.20.1.N / local: 127.0.0.1)">
              <input value={a.server.ip ?? ''} placeholder="auto" onChange={(e) => setServerField('ip', e.target.value)} />
            </Field>
          </div>
        </div>

        {/* TLS */}
        {!isTcp && (
          <Field label="TLS">
            <label style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <input type="checkbox" checked={a.tls ?? false}
                onChange={(e) => setA((prev) => ({ ...prev, tls: e.target.checked }))}
                style={{ width: 'auto' }} />
              Enable TLS (self-signed cert)
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
              {a.protocol === 'http2' && (
                <Field label="Max Streams" unit="BW mode">
                  <input type="number" value={(payload as HttpPayload).h2_max_concurrent_streams ?? 10}
                    onChange={(e) => setPayloadField('h2_max_concurrent_streams', Number(e.target.value))} />
                </Field>
              )}
            </Row>
          </>
        )}

        {/* VLAN */}
        <div style={{ borderTop: '1px solid #21262d', paddingTop: 8 }}>
          <label style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <input type="checkbox" checked={useVlan} onChange={(e) => setUseVlan(e.target.checked)} style={{ width: 'auto' }} />
            VLAN tagging
          </label>
          {useVlan && (
            <>
              <Row>
                <Field label="Outer VID" unit="1–4094">
                  <input type="number" min={1} max={4094} value={vlan.outer_vid}
                    onChange={(e) => setVlanField('outer_vid', Number(e.target.value))} />
                </Field>
                <Field label="Inner VID" unit="QinQ (blank=off)">
                  <input type="number" min={1} max={4094} value={vlan.inner_vid ?? ''}
                    placeholder="none"
                    onChange={(e) => setVlanField('inner_vid', e.target.value === '' ? undefined : Number(e.target.value))} />
                </Field>
              </Row>
              <Field label="Outer EtherType">
                <select value={vlan.outer_proto ?? 'dot1_q'}
                  onChange={(e) => setVlanField('outer_proto', e.target.value)}>
                  <option value="dot1_q">802.1Q (0x8100)</option>
                  <option value="dot1_ad">802.1ad QinQ (0x88a8)</option>
                </select>
              </Field>
            </>
          )}
        </div>

        {/* Load override */}
        <div style={{ borderTop: '1px solid #21262d', paddingTop: 8 }}>
          <label style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <input type="checkbox" checked={useLoadOverride} onChange={(e) => setUseLoadOverride(e.target.checked)} style={{ width: 'auto' }} />
            Override load settings for this association
          </label>
          {useLoadOverride && (
            <Row>
              <Field label="CPS per Client" unit="/s">
                <input type="number" value={loadOverride.cps_per_client ?? ''} placeholder="default"
                  onChange={(e) => setLoadField('cps_per_client', e.target.value)} />
              </Field>
              <Field label="CC per Client">
                <input type="number" value={loadOverride.cc_per_client ?? ''} placeholder="default"
                  onChange={(e) => setLoadField('cc_per_client', e.target.value)} />
              </Field>
            </Row>
          )}
        </div>

        {/* Actions */}
        <div style={{ display: 'flex', gap: 8 }}>
          <button className="btn-primary" onClick={handleSave} style={{ flex: 1 }}>Save Association</button>
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
  const [editingAssoc, setEditingAssoc] = useState<Association | null>(null)
  const [isNewAssoc, setIsNewAssoc] = useState(false)
  const [saveMsg, setSaveMsg] = useState<string | null>(null)

  const isRunning = testState === 'running' || testState === 'preparing' || testState === 'stopping' || testState === 'ramping_up'

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

  const setNetworkField = <K extends keyof NetworkConfig>(key: K, val: NetworkConfig[K]) =>
    setConfig((prev) => ({ ...prev, network: { ...prev.network, [key]: val } }))

  const setThresholdField = (key: keyof Thresholds, raw: string | boolean) => {
    const val = typeof raw === 'boolean' ? raw : (raw === '' ? undefined : Number(raw))
    setConfig((prev) => ({ ...prev, thresholds: { ...prev.thresholds, [key]: val } }))
  }

  // Estimated total load
  const estimatedCps = (() => {
    if (config.test_type !== 'cps') return null
    const cpsPerClient = config.default_load.cps_per_client ?? 100
    const totalClients = config.total_clients > 0
      ? config.total_clients
      : config.associations.reduce((sum, a) => sum + (a.client_net.count ?? 1), 0)
    return cpsPerClient * totalClients
  })()

  const estimatedCc = (() => {
    if (config.test_type !== 'cc' && config.test_type !== 'bw') return null
    const ccPerClient = config.default_load.cc_per_client ?? 50
    const totalClients = config.total_clients > 0
      ? config.total_clients
      : config.associations.reduce((sum, a) => sum + (a.client_net.count ?? 1), 0)
    return ccPerClient * totalClients
  })()

  // Associations
  const handleAddAssoc = () => {
    const newAssoc = defaultAssociation(config.associations.length)
    setIsNewAssoc(true)
    setEditingAssoc(newAssoc)
  }

  const handleEditAssoc = (assoc: Association) => {
    setIsNewAssoc(false)
    setEditingAssoc({ ...assoc })
  }

  const handleDeleteAssoc = (id: string) => {
    setConfig((prev) => ({ ...prev, associations: prev.associations.filter((a) => a.id !== id) }))
  }

  const handleSaveAssoc = (saved: Association) => {
    setConfig((prev) => {
      if (isNewAssoc) return { ...prev, associations: [...prev.associations, saved] }
      return { ...prev, associations: prev.associations.map((a) => (a.id === saved.id ? saved : a)) }
    })
    setEditingAssoc(null)
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
  const modeLabel = (m: NetworkMode) => m === 'loopback' ? 'Loopback' : m === 'namespace' ? 'Namespace' : 'External Port'

  return (
    <>
      {editingAssoc && (
        <AssociationDialog
          assoc={editingAssoc}
          onSave={handleSaveAssoc}
          onCancel={() => setEditingAssoc(null)}
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
          <Field label="Total Clients" unit="0=각 association별 count 사용">
            <input type="number" min={0} value={config.total_clients}
              onChange={(e) => setField('total_clients', Number(e.target.value))} />
          </Field>
        </Section>

        {/* DEFAULT LOAD */}
        <Section title="Default Load (per client)" defaultOpen>
          {config.test_type === 'cps' ? (
            <Field label="CPS per Client" unit="/s · 전체 CPS = clients × cps_per_client">
              <input type="number" value={config.default_load.cps_per_client ?? 100}
                onChange={(e) => setLoadField('cps_per_client', e.target.value)} />
            </Field>
          ) : (
            <Field label="CC per Client" unit="전체 CC = clients × cc_per_client">
              <input type="number" value={config.default_load.cc_per_client ?? 50}
                onChange={(e) => setLoadField('cc_per_client', e.target.value)} />
            </Field>
          )}
          {/* 예상 전체 부하 */}
          {(estimatedCps !== null || estimatedCc !== null) && (
            <div style={{ fontSize: 11, color: '#8b949e', background: '#161b22', borderRadius: 6, padding: '6px 10px' }}>
              예상 전체 부하:&nbsp;
              <span style={{ color: '#58a6ff', fontWeight: 700 }}>
                {estimatedCps !== null ? `~${estimatedCps.toLocaleString()} CPS` : `~${estimatedCc?.toLocaleString()} CC`}
              </span>
            </div>
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
          <Field label="Max In-flight per Client" unit="blank=auto">
            <input type="number" value={config.default_load.max_inflight_per_client ?? ''}
              placeholder="auto"
              onChange={(e) => setLoadField('max_inflight_per_client', e.target.value)} />
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
            임계값 초과 시 대시보드에 경고가 표시됩니다. auto_stop 활성화 시 자동 중단합니다.
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

        {/* NETWORK */}
        <Section title="Network" defaultOpen={false}>
          <Field label="Mode">
            <select value={config.network.mode}
              onChange={(e) => setNetworkField('mode', e.target.value as NetworkMode)}>
              <option value="loopback">Loopback (localhost, 개발/검증용)</option>
              <option value="namespace">Namespace (Linux netns, CAP_NET_ADMIN 필요)</option>
              <option value="external_port">External Port (Phase 11, 미구현)</option>
            </select>
          </Field>
          {config.network.mode === 'namespace' && (
            <Field label="NS Prefix">
              <input value={config.network.ns.netns_prefix}
                onChange={(e) => setNetworkField('ns', { ...config.network.ns, netns_prefix: e.target.value })} />
            </Field>
          )}
          {config.network.mode === 'external_port' && (
            <div style={{ fontSize: 11, color: '#d29922', padding: '6px 10px', background: '#161b22', borderRadius: 6 }}>
              ⚠ External Port 모드는 Phase 11에서 구현 예정입니다.
            </div>
          )}
          <label style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <input type="checkbox" checked={config.network.tcp_quickack}
              onChange={(e) => setNetworkField('tcp_quickack', e.target.checked)}
              style={{ width: 'auto' }} />
            TCP_QUICKACK (disable delayed ACK)
          </label>
        </Section>

        {/* ASSOCIATIONS */}
        <Section title={`Associations (${config.associations.length}) — Mode: ${modeLabel(config.network.mode)}`} defaultOpen>
          <div style={{ overflowX: 'auto' }}>
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
              <thead>
                <tr style={{ color: '#8b949e', textAlign: 'left' }}>
                  <th style={{ padding: '4px 6px' }}>Name</th>
                  <th style={{ padding: '4px 6px' }}>Protocol</th>
                  <th style={{ padding: '4px 6px' }}>Client Net</th>
                  <th style={{ padding: '4px 6px' }}>Server</th>
                  <th style={{ padding: '4px 6px' }}>Load</th>
                  <th style={{ padding: '4px 6px', width: 90 }}></th>
                </tr>
              </thead>
              <tbody>
                {config.associations.map((assoc) => (
                  <tr key={assoc.id} style={{ borderTop: '1px solid #21262d' }}>
                    <td style={{ padding: '5px 6px', color: '#e6edf3' }}>
                      {assoc.name || assoc.id.slice(0, 8)}
                      {assoc.vlan && <span style={{ fontSize: 10, color: '#bc8cff', marginLeft: 4 }}>VLAN {assoc.vlan.outer_vid}</span>}
                    </td>
                    <td style={{ padding: '5px 6px', fontWeight: 600, color: '#58a6ff' }}>
                      {protoLabel(assoc.protocol)}
                      {assoc.tls && <span style={{ fontSize: 10, color: '#d29922', marginLeft: 4 }}>TLS</span>}
                    </td>
                    <td style={{ padding: '5px 6px', color: '#8b949e', fontFamily: 'monospace', fontSize: 11 }}>
                      {assoc.client_net.base_ip}
                      {assoc.client_net.count ? `×${assoc.client_net.count}` : '×auto'}
                    </td>
                    <td style={{ padding: '5px 6px', color: '#8b949e' }}>
                      {assoc.server.id} — {assoc.server.ip ?? '0.0.0.0'}:{assoc.server.port}
                    </td>
                    <td style={{ padding: '5px 6px', color: '#484f58' }}>
                      {assoc.load ? 'custom' : 'default'}
                    </td>
                    <td style={{ padding: '5px 6px' }}>
                      <div style={{ display: 'flex', gap: 4 }}>
                        <button className="btn-secondary" onClick={() => handleEditAssoc(assoc)}
                          style={{ padding: '2px 8px', fontSize: 11 }}>Edit</button>
                        <button className="btn-danger" onClick={() => handleDeleteAssoc(assoc.id)}
                          disabled={config.associations.length <= 1}
                          style={{ padding: '2px 8px', fontSize: 11 }}>✕</button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <button className="btn-secondary" onClick={handleAddAssoc}
            style={{ alignSelf: 'flex-start', padding: '4px 12px', fontSize: 12 }}>
            + Add Association
          </button>
        </Section>

        {/* Start / Stop */}
        <div style={{ display: 'flex', gap: 8, marginTop: 4 }}>
          <button className="btn-primary" onClick={() => startTest(config)}
            disabled={isRunning || config.associations.length === 0} style={{ flex: 1 }}>
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
