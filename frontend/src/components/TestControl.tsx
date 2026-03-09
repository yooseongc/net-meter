import { useState, useEffect, useRef } from 'react'
import { Save, Download, Upload, Plus, Pencil, Trash2, ChevronDown, ChevronRight } from 'lucide-react'
import {
  TestConfig, TestType, Protocol, HttpMethod,
  Association, ClientDef, ServerDef,
  PayloadProfile, LoadConfig, NetworkConfig,
  TcpPayload, HttpPayload, Thresholds, VlanConfig,
} from '../api/client'
import { useTestStore } from '../store/testStore'
import { v4 as uuidv4 } from 'uuid'
import { Card, CardContent, CardTitle } from './ui/card'
import { Button } from './ui/button'
import { Badge } from './ui/badge'
import { Input, NativeSelect } from './ui/input'
import { Label } from './ui/label'
import { Switch } from './ui/switch'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from './ui/dialog'
import { cn } from '@/lib/utils'

// ─── Defaults ─────────────────────────────────────────────────────────────────

const defaultHttpPayload = (): HttpPayload => ({ type: 'http', method: 'GET', path: '/' })
const defaultTcpPayload = (): TcpPayload => ({ type: 'tcp', tx_bytes: 64, rx_bytes: 64 })
const defaultPayloadForProtocol = (proto: Protocol): PayloadProfile =>
  proto === 'tcp' ? defaultTcpPayload() : defaultHttpPayload()

const defaultClientDef = (idx: number): ClientDef => ({
  id: uuidv4(),
  name: `client-${idx}`,
  cidr: `10.10.${idx + 1}.1/24`,
})

const defaultServerDef = (idx: number): ServerDef => ({
  id: uuidv4(),
  name: `server-${idx}`,
  port: 8080,
  protocol: 'http1',
  tls: false,
})

const defaultAssociation = (clients: ClientDef[], servers: ServerDef[], idx: number): Association => ({
  id: uuidv4(),
  name: `assoc-${idx}`,
  client_id: clients[0]?.id ?? '',
  server_id: servers[0]?.id ?? '',
  payload: defaultHttpPayload(),
})

const defaultNetworkConfig = (): NetworkConfig => ({
  tcp_quickack: false,
})

const makeDefaultConfig = (): TestConfig => {
  const client = defaultClientDef(0)
  const server = defaultServerDef(0)
  return {
    id: uuidv4(),
    name: 'New Test',
    test_type: 'cps',
    duration_secs: 30,
    default_load: { num_connections: 100, connect_timeout_ms: 5000, response_timeout_ms: 30000, ramp_up_secs: 0 },
    clients: [client],
    servers: [server],
    associations: [{
      id: uuidv4(),
      name: 'assoc-0',
      client_id: client.id,
      server_id: server.id,
      payload: defaultHttpPayload(),
    }],
    network: defaultNetworkConfig(),
    thresholds: {},
  }
}

// ─── Form helpers ─────────────────────────────────────────────────────────────

function Section({ title, children, defaultOpen = true }: {
  title: string; children: React.ReactNode; defaultOpen?: boolean
}) {
  const [open, setOpen] = useState(defaultOpen)
  return (
    <div className="border-t border-border pt-3">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-wider text-muted-foreground mb-2 w-full text-left hover:text-foreground transition-colors"
      >
        {open
          ? <ChevronDown className="h-3.5 w-3.5 text-primary" />
          : <ChevronRight className="h-3.5 w-3.5 text-primary" />}
        {title}
      </button>
      {open && <div className="flex flex-col gap-2.5">{children}</div>}
    </div>
  )
}

function Row({ children }: { children: React.ReactNode }) {
  return <div className="grid grid-cols-2 gap-2">{children}</div>
}

function Field({ label, unit, children }: { label: string; unit?: string; children: React.ReactNode }) {
  return (
    <div>
      <Label className="flex justify-between">
        <span>{label}</span>
        {unit && <span className="text-muted-foreground/50 text-[10px]">{unit}</span>}
      </Label>
      {children}
    </div>
  )
}

// ─── ClientDef Dialog ──────────────────────────────────────────────────────────

function ClientDialog({ client, open, onSave, onCancel }: {
  client: ClientDef; open: boolean
  onSave: (c: ClientDef) => void; onCancel: () => void
}) {
  const [c, setC] = useState<ClientDef>({ ...client })
  useEffect(() => { setC({ ...client }) }, [client])

  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v) onCancel() }}>
      <DialogContent className="max-w-[480px]">
        <DialogHeader>
          <DialogTitle>Edit Client</DialogTitle>
        </DialogHeader>
        <Field label="Name">
          <Input value={c.name} onChange={(e) => setC((p) => ({ ...p, name: e.target.value }))} />
        </Field>
        <Field label="CIDR" unit='e.g. "10.10.1.1/24"'>
          <Input value={c.cidr} placeholder="10.10.1.1/24"
            onChange={(e) => setC((p) => ({ ...p, cidr: e.target.value }))} />
        </Field>
        <div className="flex gap-2 pt-1">
          <Button onClick={() => onSave(c)} className="flex-1">Save Client</Button>
          <Button variant="secondary" onClick={onCancel} className="flex-1">Cancel</Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

// ─── ServerDef Dialog ─────────────────────────────────────────────────────────

function ServerDialog({ server, open, onSave, onCancel }: {
  server: ServerDef; open: boolean
  onSave: (s: ServerDef) => void; onCancel: () => void
}) {
  const [s, setS] = useState<ServerDef>({ ...server })
  useEffect(() => { setS({ ...server }) }, [server])

  const isTcp = s.protocol === 'tcp'

  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v) onCancel() }}>
      <DialogContent className="max-w-[480px]">
        <DialogHeader>
          <DialogTitle>Edit Server</DialogTitle>
        </DialogHeader>
        <Row>
          <Field label="Name">
            <Input value={s.name} onChange={(e) => setS((p) => ({ ...p, name: e.target.value }))} />
          </Field>
          <Field label="Protocol">
            <NativeSelect value={s.protocol} onChange={(e) => setS((p) => ({ ...p, protocol: e.target.value as Protocol, tls: false }))}>
              <option value="tcp">TCP</option>
              <option value="http1">HTTP/1.1</option>
              <option value="http2">HTTP/2</option>
            </NativeSelect>
          </Field>
        </Row>
        <Row>
          <Field label="Port">
            <Input type="number" min={1} max={65535} value={s.port}
              onChange={(e) => setS((p) => ({ ...p, port: Number(e.target.value) }))} />
          </Field>
          <Field label="Server IP" unit="blank=auto">
            <Input value={s.ip ?? ''} placeholder="auto (127.0.0.1 / NS: 10.20.1.N)"
              onChange={(e) => setS((p) => ({ ...p, ip: e.target.value || undefined }))} />
          </Field>
        </Row>
        {!isTcp && (
          <>
            <label className="flex items-center gap-2.5 text-sm cursor-pointer">
              <Switch checked={s.tls ?? false} onCheckedChange={(v) => setS((p) => ({ ...p, tls: v }))} />
              Enable TLS (self-signed cert)
            </label>
            {(s.tls ?? false) && (
              <Field label="TLS Server Name (SNI)">
                <Input
                  value={s.tls_server_name ?? 'test.net-meter.com'}
                  placeholder="test.net-meter.com"
                  onChange={(e) => setS((p) => ({ ...p, tls_server_name: e.target.value || 'test.net-meter.com' }))}
                />
                <p className="text-xs text-muted-foreground mt-1">IP 주소 입력 시 자동으로 &quot;localhost&quot;로 대체됩니다.</p>
              </Field>
            )}
          </>
        )}
        <div className="flex gap-2 pt-1">
          <Button onClick={() => onSave(s)} className="flex-1">Save Server</Button>
          <Button variant="secondary" onClick={onCancel} className="flex-1">Cancel</Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

// ─── Association Dialog ───────────────────────────────────────────────────────

function AssociationDialog({
  assoc, clients, servers, open, onSave, onCancel,
}: {
  assoc: Association; clients: ClientDef[]; servers: ServerDef[]
  open: boolean; onSave: (a: Association) => void; onCancel: () => void
}) {
  const [a, setA] = useState<Association>({ ...assoc })
  useEffect(() => { setA({ ...assoc }) }, [assoc])

  const selectedServer = servers.find((s) => s.id === a.server_id)
  const isTcp = selectedServer?.protocol === 'tcp'

  // Payload 업데이트: 서버 protocol 변경 시 초기화
  const setServerId = (id: string) => {
    const sv = servers.find((s) => s.id === id)
    if (!sv) return
    setA((prev) => ({
      ...prev,
      server_id: id,
      payload: defaultPayloadForProtocol(sv.protocol),
    }))
  }

  const setPayloadField = (key: string, val: unknown) =>
    setA((prev) => ({ ...prev, payload: { ...prev.payload, [key]: val } as PayloadProfile }))

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
    onSave({ ...a, load: useLoadOverride ? loadOverride : undefined, vlan: useVlan ? vlan : undefined })
  }

  const payload = a.payload as (TcpPayload & { type?: string }) | (HttpPayload & { type?: string })

  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v) onCancel() }}>
      <DialogContent className="max-w-[560px]">
        <DialogHeader>
          <DialogTitle>Edit Association</DialogTitle>
        </DialogHeader>

        <Row>
          <Field label="Name">
            <Input value={a.name} onChange={(e) => setA((p) => ({ ...p, name: e.target.value }))} />
          </Field>
          <Field label="Client">
            <NativeSelect value={a.client_id} onChange={(e) => setA((p) => ({ ...p, client_id: e.target.value }))}>
              {clients.map((c) => (
                <option key={c.id} value={c.id}>{c.name} ({c.cidr})</option>
              ))}
            </NativeSelect>
          </Field>
        </Row>
        <Field label="Server">
          <NativeSelect value={a.server_id} onChange={(e) => setServerId(e.target.value)}>
            {servers.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name} — {s.protocol.toUpperCase()}{s.tls ? '+TLS' : ''} :{s.port}
              </option>
            ))}
          </NativeSelect>
        </Field>

        {/* Payload */}
        {isTcp ? (
          <div>
            <div className="text-[11px] font-bold uppercase text-muted-foreground mb-2">TCP Payload</div>
            <Row>
              <Field label="TX bytes" unit="client→server">
                <Input type="number" value={(payload as TcpPayload).tx_bytes ?? 0}
                  onChange={(e) => setPayloadField('tx_bytes', Number(e.target.value))} />
              </Field>
              <Field label="RX bytes" unit="server→client">
                <Input type="number" value={(payload as TcpPayload).rx_bytes ?? 0}
                  onChange={(e) => setPayloadField('rx_bytes', Number(e.target.value))} />
              </Field>
            </Row>
          </div>
        ) : (
          <div>
            <div className="text-[11px] font-bold uppercase text-muted-foreground mb-2">HTTP Payload</div>
            <Row>
              <Field label="Method">
                <NativeSelect value={(payload as HttpPayload).method ?? 'GET'}
                  onChange={(e) => setPayloadField('method', e.target.value as HttpMethod)}>
                  <option value="GET">GET</option>
                  <option value="POST">POST</option>
                </NativeSelect>
              </Field>
              <Field label="Path">
                <Input value={(payload as HttpPayload).path ?? '/'}
                  onChange={(e) => setPayloadField('path', e.target.value)} />
              </Field>
            </Row>
            <Row>
              <Field label="Request Body" unit="bytes">
                <Input type="number" value={(payload as HttpPayload).request_body_bytes ?? ''} placeholder="none"
                  onChange={(e) => setPayloadField('request_body_bytes', e.target.value === '' ? undefined : Number(e.target.value))} />
              </Field>
              <Field label="Response Body" unit="bytes">
                <Input type="number" value={(payload as HttpPayload).response_body_bytes ?? ''} placeholder="none"
                  onChange={(e) => setPayloadField('response_body_bytes', e.target.value === '' ? undefined : Number(e.target.value))} />
              </Field>
            </Row>
            <Row>
              <Field label="URL Padding" unit="bytes">
                <Input type="number" value={(payload as HttpPayload).path_extra_bytes ?? ''} placeholder="none"
                  onChange={(e) => setPayloadField('path_extra_bytes', e.target.value === '' ? undefined : Number(e.target.value))} />
              </Field>
              {selectedServer?.protocol === 'http2' && (
                <Field label="Max Streams" unit="BW mode">
                  <Input type="number" value={(payload as HttpPayload).h2_max_concurrent_streams ?? 10}
                    onChange={(e) => setPayloadField('h2_max_concurrent_streams', Number(e.target.value))} />
                </Field>
              )}
            </Row>
          </div>
        )}

        {/* VLAN */}
        <div className="border-t border-border pt-3">
          <label className="flex items-center gap-2.5 text-sm cursor-pointer mb-3">
            <Switch checked={useVlan} onCheckedChange={setUseVlan} />
            VLAN tagging
          </label>
          {useVlan && (
            <>
              <Row>
                <Field label="Outer VID" unit="1–4094">
                  <Input type="number" min={1} max={4094} value={vlan.outer_vid}
                    onChange={(e) => setVlanField('outer_vid', Number(e.target.value))} />
                </Field>
                <Field label="Inner VID" unit="QinQ (blank=off)">
                  <Input type="number" min={1} max={4094} value={vlan.inner_vid ?? ''} placeholder="none"
                    onChange={(e) => setVlanField('inner_vid', e.target.value === '' ? undefined : Number(e.target.value))} />
                </Field>
              </Row>
              <div className="mt-2">
                <Field label="Outer EtherType">
                  <NativeSelect value={vlan.outer_proto ?? 'dot1_q'}
                    onChange={(e) => setVlanField('outer_proto', e.target.value)}>
                    <option value="dot1_q">802.1Q (0x8100)</option>
                    <option value="dot1_ad">802.1ad QinQ (0x88a8)</option>
                  </NativeSelect>
                </Field>
              </div>
            </>
          )}
        </div>

        {/* Load override */}
        <div className="border-t border-border pt-3">
          <label className="flex items-center gap-2.5 text-sm cursor-pointer mb-3">
            <Switch checked={useLoadOverride} onCheckedChange={setUseLoadOverride} />
            Override load settings for this association
          </label>
          {useLoadOverride && (
            <Field label="Total Connections" unit="CPS: 병렬 루프 수(총) / CC·BW: 동시 연결 수(총)">
              <Input type="number" min={1} value={loadOverride.num_connections ?? ''} placeholder="default"
                onChange={(e) => setLoadField('num_connections', e.target.value)} />
            </Field>
          )}
        </div>

        {/* Actions */}
        <div className="flex gap-2 pt-1">
          <Button onClick={handleSave} className="flex-1">Save Association</Button>
          <Button variant="secondary" onClick={onCancel} className="flex-1">Cancel</Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

// ─── Auto-save (B-2) ──────────────────────────────────────────────────────────

const DRAFT_KEY = 'net-meter-draft-config'

function isValidConfig(c: unknown): c is TestConfig {
  if (!c || typeof c !== 'object') return false
  const o = c as Record<string, unknown>
  return Array.isArray(o.clients) && Array.isArray(o.servers) && Array.isArray(o.associations)
}

function loadDraft(): TestConfig | null {
  try {
    const raw = localStorage.getItem(DRAFT_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw)
    return isValidConfig(parsed) ? parsed : null
  } catch {
    return null
  }
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function TestControl() {
  const { savedProfiles, saveProfile, draftConfig, setDraftConfig } = useTestStore()
  const [config, setConfig] = useState<TestConfig>(() => loadDraft() ?? makeDefaultConfig())
  const [editingClient, setEditingClient] = useState<ClientDef | null>(null)
  const [isNewClient, setIsNewClient] = useState(false)
  const [editingServer, setEditingServer] = useState<ServerDef | null>(null)
  const [isNewServer, setIsNewServer] = useState(false)
  const [editingAssoc, setEditingAssoc] = useState<Association | null>(null)
  const [isNewAssoc, setIsNewAssoc] = useState(false)
  const [saveMsg, setSaveMsg] = useState<string | null>(null)
  const autoSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  // draftConfig: Profiles 탭에서 Config 탭으로 전달된 설정 로드
  useEffect(() => {
    if (draftConfig) {
      setConfig({ ...draftConfig })
      setDraftConfig(null)
    }
  }, [draftConfig])

  // B-2: config 변경 시 debounce 자동저장
  useEffect(() => {
    if (autoSaveTimer.current) clearTimeout(autoSaveTimer.current)
    autoSaveTimer.current = setTimeout(() => {
      try {
        localStorage.setItem(DRAFT_KEY, JSON.stringify(config))
      } catch (e) {
        // QuotaExceededError 등 localStorage 쓰기 실패 — 조용히 넘어감
        console.warn('[net-meter] Auto-save failed (localStorage quota exceeded?):', e)
      }
    }, 500)
    return () => {
      if (autoSaveTimer.current) clearTimeout(autoSaveTimer.current)
    }
  }, [config])

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

  const numConnections = config.default_load.num_connections ?? 100

  // ─ Client handlers ─
  const handleAddClient = () => { setIsNewClient(true); setEditingClient(defaultClientDef(config.clients.length)) }
  const handleEditClient = (c: ClientDef) => { setIsNewClient(false); setEditingClient({ ...c }) }
  const handleDeleteClient = (id: string) => {
    setConfig((prev) => ({
      ...prev,
      clients: prev.clients.filter((c) => c.id !== id),
      associations: prev.associations.filter((a) => a.client_id !== id),
    }))
  }
  const handleSaveClient = (saved: ClientDef) => {
    setConfig((prev) => {
      if (isNewClient) return { ...prev, clients: [...prev.clients, saved] }
      return { ...prev, clients: prev.clients.map((c) => (c.id === saved.id ? saved : c)) }
    })
    setEditingClient(null)
  }

  // ─ Server handlers ─
  const handleAddServer = () => { setIsNewServer(true); setEditingServer(defaultServerDef(config.servers.length)) }
  const handleEditServer = (s: ServerDef) => { setIsNewServer(false); setEditingServer({ ...s }) }
  const handleDeleteServer = (id: string) => {
    setConfig((prev) => ({
      ...prev,
      servers: prev.servers.filter((s) => s.id !== id),
      associations: prev.associations.filter((a) => a.server_id !== id),
    }))
  }
  const handleSaveServer = (saved: ServerDef) => {
    setConfig((prev) => {
      if (isNewServer) return { ...prev, servers: [...prev.servers, saved] }
      return { ...prev, servers: prev.servers.map((s) => (s.id === saved.id ? saved : s)) }
    })
    setEditingServer(null)
  }

  // ─ Association handlers ─
  const handleAddAssoc = () => {
    setIsNewAssoc(true)
    setEditingAssoc(defaultAssociation(config.clients, config.servers, config.associations.length))
  }
  const handleEditAssoc = (assoc: Association) => { setIsNewAssoc(false); setEditingAssoc({ ...assoc }) }
  const handleDeleteAssoc = (id: string) =>
    setConfig((prev) => ({ ...prev, associations: prev.associations.filter((a) => a.id !== id) }))
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

  const protoLabel = (p: string) => p === 'tcp' ? 'TCP' : p === 'http1' ? 'HTTP/1.1' : 'HTTP/2'

  const clientName = (id: string) => config.clients.find((c) => c.id === id)?.name ?? id.slice(0, 8)
  const serverName = (id: string) => {
    const s = config.servers.find((sv) => sv.id === id)
    return s ? `${s.name} (${protoLabel(s.protocol)}:${s.port})` : id.slice(0, 8)
  }

  return (
    <>
      {/* Dialogs */}
      {editingClient && (
        <ClientDialog
          client={editingClient}
          open={!!editingClient}
          onSave={handleSaveClient}
          onCancel={() => setEditingClient(null)}
        />
      )}
      {editingServer && (
        <ServerDialog
          server={editingServer}
          open={!!editingServer}
          onSave={handleSaveServer}
          onCancel={() => setEditingServer(null)}
        />
      )}
      {editingAssoc && (
        <AssociationDialog
          assoc={editingAssoc}
          clients={config.clients}
          servers={config.servers}
          open={!!editingAssoc}
          onSave={handleSaveAssoc}
          onCancel={() => setEditingAssoc(null)}
        />
      )}

      <Card>
        <CardContent className="flex flex-col gap-3">
          {/* Header */}
          <div className="flex justify-between items-center">
            <CardTitle>Test Config</CardTitle>
            <div className="flex items-center gap-1.5">
              {saveMsg && <span className="text-xs text-success">{saveMsg}</span>}
              <Button
                variant="secondary" size="xs"
                onClick={() => {
                  saveProfile(config)
                  setSaveMsg('Saved!')
                  setTimeout(() => setSaveMsg(null), 2000)
                }}
              >
                <Save className="h-3 w-3" />
                Save to Profiles
              </Button>
              <Button variant="secondary" size="xs" onClick={exportConfig}>
                <Download className="h-3 w-3" />
                Export
              </Button>
              <label className={cn(
                'inline-flex items-center gap-1 h-6 px-2 text-xs rounded-[var(--radius)] font-semibold cursor-pointer',
                'bg-secondary text-secondary-foreground border border-border hover:bg-muted transition-colors',
              )}>
                <Upload className="h-3 w-3" />
                Import
                <input type="file" accept=".json" onChange={importConfig} className="hidden" />
              </label>
            </div>
          </div>

          {/* Load saved config */}
          {savedProfiles.length > 0 && (
            <div>
              <Label>Load Saved Config</Label>
              <NativeSelect onChange={(e) => loadProfile(e.target.value)} defaultValue="">
                <option value="" disabled>Select…</option>
                {savedProfiles.map((p) => (
                  <option key={p.id} value={p.id}>{p.name}</option>
                ))}
              </NativeSelect>
            </div>
          )}

          {/* Basic */}
          <Section title="Basic" defaultOpen>
            <Field label="Config Name">
              <Input value={config.name} onChange={(e) => setField('name', e.target.value)} />
            </Field>
            <Row>
              <Field label="Test Type">
                <NativeSelect value={config.test_type} onChange={(e) => setField('test_type', e.target.value as TestType)}>
                  <option value="cps">CPS — 연결 최대 속도</option>
                  <option value="cc">CC — 동시 연결 유지</option>
                  <option value="bw">BW — 대역폭 포화</option>
                </NativeSelect>
              </Field>
              <Field label="Duration" unit="sec (0=manual)">
                <Input type="number" value={config.duration_secs}
                  onChange={(e) => setField('duration_secs', Number(e.target.value))} />
              </Field>
            </Row>
          </Section>

          {/* Default Load */}
          <Section title="Default Load" defaultOpen>
            <Field
              label="Total Clients"
              unit={config.test_type === 'cps' ? 'CPS: 병렬 루프 수' : 'CC·BW: 동시 연결 수'}
            >
              <Input type="number" min={1} value={numConnections}
                onChange={(e) => setLoadField('num_connections', e.target.value)} />
            </Field>
            <div className="text-xs text-muted-foreground bg-subtle rounded px-3 py-2">
              {config.test_type === 'cps'
                ? `CPS: ${numConnections.toLocaleString()}개 클라이언트가 connect→transact→close 루프를 최대 속도로 반복합니다. 실제 CPS는 측정값으로 확인하세요.`
                : `CC·BW: ${numConnections.toLocaleString()}개 연결을 동시에 유지합니다. 각 association에 균등 분배됩니다.`}
            </div>
            <Row>
              <Field label="Connect Timeout" unit="ms">
                <Input type="number" value={config.default_load.connect_timeout_ms ?? 5000}
                  onChange={(e) => setLoadField('connect_timeout_ms', e.target.value)} />
              </Field>
              <Field label="Response Timeout" unit="ms">
                <Input type="number" value={config.default_load.response_timeout_ms ?? 30000}
                  onChange={(e) => setLoadField('response_timeout_ms', e.target.value)} />
              </Field>
            </Row>
            <Row>
              <Field label="Ramp-up" unit="sec (0=off)">
                <Input type="number" value={config.default_load.ramp_up_secs ?? 0} min={0}
                  onChange={(e) => setLoadField('ramp_up_secs', e.target.value)} />
              </Field>
              <Field label="Ramp-down" unit="sec (0=off)">
                <Input type="number" value={config.default_load.ramp_down_secs ?? 0} min={0}
                  onChange={(e) => setLoadField('ramp_down_secs', e.target.value)} />
              </Field>
            </Row>
          </Section>

          {/* Thresholds */}
          <Section title="Thresholds / Alarm" defaultOpen={false}>
            <p className="text-xs text-muted-foreground">
              임계값 초과 시 대시보드에 경고가 표시됩니다. auto_stop 활성화 시 자동 중단합니다.
            </p>
            <Row>
              <Field label="Min CPS" unit="/s (blank=off)">
                <Input type="number" value={config.thresholds?.min_cps ?? ''} placeholder="none"
                  onChange={(e) => setThresholdField('min_cps', e.target.value)} />
              </Field>
              <Field label="Max Error Rate" unit="% (blank=off)">
                <Input type="number" value={config.thresholds?.max_error_rate_pct ?? ''} placeholder="none"
                  onChange={(e) => setThresholdField('max_error_rate_pct', e.target.value)} />
              </Field>
            </Row>
            <Field label="Max Latency p99" unit="ms (blank=off)">
              <Input type="number" value={config.thresholds?.max_latency_p99_ms ?? ''} placeholder="none"
                onChange={(e) => setThresholdField('max_latency_p99_ms', e.target.value)} />
            </Field>
            <label className="flex items-center gap-2.5 text-sm cursor-pointer">
              <Switch
                checked={config.thresholds?.auto_stop_on_fail ?? false}
                onCheckedChange={(v) => setThresholdField('auto_stop_on_fail', v)}
              />
              Auto-stop on violation
            </label>
          </Section>

          {/* Network */}
          <Section title="Network" defaultOpen={false}>
            <p className="text-xs text-muted-foreground">
              네트워크 모드(loopback / namespace / external_port)는 서버 시작 시 CLI 인수로 결정됩니다.
            </p>
            <label className="flex items-center gap-2.5 text-sm cursor-pointer">
              <Switch
                checked={config.network.tcp_quickack ?? false}
                onCheckedChange={(v) => setNetworkField('tcp_quickack', v)}
              />
              TCP_QUICKACK (disable delayed ACK)
            </label>
          </Section>

          {/* Clients */}
          <Section title={`Clients (${config.clients.length})`} defaultOpen>
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-xs">
                <thead>
                  <tr className="text-muted-foreground text-[11px] border-b border-border">
                    <th className="text-left px-2 py-1.5">Name</th>
                    <th className="text-left px-2 py-1.5">CIDR</th>
                    <th className="px-2 py-1.5 w-20"></th>
                  </tr>
                </thead>
                <tbody>
                  {config.clients.map((client) => (
                    <tr key={client.id} className="border-t border-border hover:bg-muted/30 transition-colors">
                      <td className="px-2 py-1.5 font-medium text-foreground">{client.name}</td>
                      <td className="px-2 py-1.5 font-mono text-muted-foreground">{client.cidr}</td>
                      <td className="px-2 py-1.5">
                        <div className="flex gap-1">
                          <Button variant="secondary" size="xs" onClick={() => handleEditClient(client)}>
                            <Pencil className="h-3 w-3" />
                          </Button>
                          <Button variant="destructive" size="xs"
                            disabled={config.clients.length <= 1}
                            onClick={() => handleDeleteClient(client.id)}>
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <Button variant="secondary" size="sm" onClick={handleAddClient} className="self-start">
              <Plus className="h-3.5 w-3.5" />
              Add Client
            </Button>
          </Section>

          {/* Servers */}
          <Section title={`Servers (${config.servers.length})`} defaultOpen>
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-xs">
                <thead>
                  <tr className="text-muted-foreground text-[11px] border-b border-border">
                    <th className="text-left px-2 py-1.5">Name</th>
                    <th className="text-left px-2 py-1.5">Protocol</th>
                    <th className="text-left px-2 py-1.5">IP:Port</th>
                    <th className="px-2 py-1.5 w-20"></th>
                  </tr>
                </thead>
                <tbody>
                  {config.servers.map((server) => (
                    <tr key={server.id} className="border-t border-border hover:bg-muted/30 transition-colors">
                      <td className="px-2 py-1.5 font-medium text-foreground">{server.name}</td>
                      <td className="px-2 py-1.5 font-semibold text-primary">
                        {protoLabel(server.protocol)}
                        {server.tls && <Badge variant="warning" className="ml-1.5">TLS</Badge>}
                      </td>
                      <td className="px-2 py-1.5 text-muted-foreground font-mono">
                        {server.ip ?? '0.0.0.0'}:{server.port}
                      </td>
                      <td className="px-2 py-1.5">
                        <div className="flex gap-1">
                          <Button variant="secondary" size="xs" onClick={() => handleEditServer(server)}>
                            <Pencil className="h-3 w-3" />
                          </Button>
                          <Button variant="destructive" size="xs"
                            disabled={config.servers.length <= 1}
                            onClick={() => handleDeleteServer(server.id)}>
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <Button variant="secondary" size="sm" onClick={handleAddServer} className="self-start">
              <Plus className="h-3.5 w-3.5" />
              Add Server
            </Button>
          </Section>

          {/* Associations */}
          <Section title={`Associations (${config.associations.length})`} defaultOpen>
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-xs">
                <thead>
                  <tr className="text-muted-foreground text-[11px] border-b border-border">
                    <th className="text-left px-2 py-1.5">Name</th>
                    <th className="text-left px-2 py-1.5">Client</th>
                    <th className="text-left px-2 py-1.5">Server</th>
                    <th className="text-left px-2 py-1.5">Load</th>
                    <th className="px-2 py-1.5 w-20"></th>
                  </tr>
                </thead>
                <tbody>
                  {config.associations.map((assoc) => (
                    <tr key={assoc.id} className="border-t border-border hover:bg-muted/30 transition-colors">
                      <td className="px-2 py-1.5 text-foreground">
                        {assoc.name || assoc.id.slice(0, 8)}
                        {assoc.vlan && <Badge variant="purple" className="ml-1.5">VLAN {assoc.vlan.outer_vid}</Badge>}
                      </td>
                      <td className="px-2 py-1.5 text-muted-foreground">{clientName(assoc.client_id)}</td>
                      <td className="px-2 py-1.5 text-muted-foreground">{serverName(assoc.server_id)}</td>
                      <td className="px-2 py-1.5 text-muted-foreground/50">
                        {assoc.load ? `×${assoc.load.num_connections ?? 1}` : 'default'}
                      </td>
                      <td className="px-2 py-1.5">
                        <div className="flex gap-1">
                          <Button variant="secondary" size="xs" onClick={() => handleEditAssoc(assoc)}>
                            <Pencil className="h-3 w-3" />
                          </Button>
                          <Button variant="destructive" size="xs"
                            disabled={config.associations.length <= 1}
                            onClick={() => handleDeleteAssoc(assoc.id)}>
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <Button variant="secondary" size="sm" onClick={handleAddAssoc} className="self-start"
              disabled={config.clients.length === 0 || config.servers.length === 0}>
              <Plus className="h-3.5 w-3.5" />
              Add Association
            </Button>
          </Section>

        </CardContent>
      </Card>
    </>
  )
}
