import { useState, useEffect, useRef } from 'react'
import { Save, Download, Upload, Plus, Pencil, Trash2, ChevronDown, ChevronRight } from 'lucide-react'
import {
  TestConfig, TestType, Protocol, HttpMethod,
  Association, PayloadProfile, LoadConfig, NetworkConfig, NetworkMode,
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

// ─── Association Dialog ───────────────────────────────────────────────────────

function AssociationDialog({
  assoc, open, onSave, onCancel,
}: {
  assoc: Association; open: boolean
  onSave: (a: Association) => void; onCancel: () => void
}) {
  const [a, setA] = useState<Association>({ ...assoc })

  useEffect(() => { setA({ ...assoc }) }, [assoc])

  const setProtocol = (proto: Protocol) =>
    setA((prev) => ({ ...prev, protocol: proto, payload: defaultPayloadForProtocol(proto) }))

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
    onSave({ ...a, load: useLoadOverride ? loadOverride : undefined, vlan: useVlan ? vlan : undefined })
  }

  const isTcp = a.protocol === 'tcp'
  const payload = a.payload as (TcpPayload & { type?: string }) | (HttpPayload & { type?: string })

  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v) onCancel() }}>
      <DialogContent className="max-w-[560px]">
        <DialogHeader>
          <DialogTitle>Edit Association</DialogTitle>
        </DialogHeader>

        {/* Name & Protocol */}
        <Row>
          <Field label="Name">
            <Input value={a.name} onChange={(e) => setA((prev) => ({ ...prev, name: e.target.value }))} />
          </Field>
          <Field label="Protocol">
            <NativeSelect value={a.protocol} onChange={(e) => setProtocol(e.target.value as Protocol)}>
              <option value="tcp">TCP</option>
              <option value="http1">HTTP/1.1</option>
              <option value="http2">HTTP/2</option>
            </NativeSelect>
          </Field>
        </Row>

        {/* Client Net */}
        <div>
          <div className="text-[11px] font-bold text-primary uppercase mb-2">Client IP Range</div>
          <Row>
            <Field label="Base IP">
              <Input value={a.client_net.base_ip} placeholder="10.10.1.1"
                onChange={(e) => setClientNetField('base_ip', e.target.value)} />
            </Field>
            <Field label="Count (workers)" unit="blank=auto">
              <Input type="number" min={1} value={a.client_net.count ?? ''} placeholder="auto"
                onChange={(e) => setClientNetField('count', e.target.value === '' ? undefined : Math.max(1, Number(e.target.value)))} />
            </Field>
          </Row>
          <div className="mt-2">
            <Field label="Prefix Length" unit="/24">
              <Input type="number" min={8} max={32} value={a.client_net.prefix_len ?? 24}
                onChange={(e) => setClientNetField('prefix_len', Number(e.target.value))} />
            </Field>
          </div>
          <p className="text-[10px] text-muted-foreground/60 mt-1">
            {a.client_net.count
              ? `${a.client_net.count}개 워커: ${a.client_net.base_ip} ~ (base_ip+${a.client_net.count - 1})`
              : 'Count: total_clients / associations.len() 자동 계산'}
          </p>
        </div>

        {/* Server */}
        <div>
          <div className="text-[11px] font-bold text-success uppercase mb-2">Server Endpoint</div>
          <Row>
            <Field label="Server ID">
              <Input value={a.server.id} onChange={(e) => setServerField('id', e.target.value)} />
            </Field>
            <Field label="Port">
              <Input type="number" value={a.server.port} onChange={(e) => setServerField('port', Number(e.target.value))} />
            </Field>
          </Row>
          <div className="mt-2">
            <Field label="Server IP" unit="blank=auto (NS: 10.20.1.N / local: 127.0.0.1)">
              <Input value={a.server.ip ?? ''} placeholder="auto"
                onChange={(e) => setServerField('ip', e.target.value)} />
            </Field>
          </div>
        </div>

        {/* TLS */}
        {!isTcp && (
          <label className="flex items-center gap-2.5 text-sm cursor-pointer">
            <Switch
              checked={a.tls ?? false}
              onCheckedChange={(v) => setA((prev) => ({ ...prev, tls: v }))}
            />
            Enable TLS (self-signed cert)
          </label>
        )}

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
              {a.protocol === 'http2' && (
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
            <Row>
              <Field label="CPS per Client" unit="/s">
                <Input type="number" value={loadOverride.cps_per_client ?? ''} placeholder="default"
                  onChange={(e) => setLoadField('cps_per_client', e.target.value)} />
              </Field>
              <Field label="CC per Client">
                <Input type="number" value={loadOverride.cc_per_client ?? ''} placeholder="default"
                  onChange={(e) => setLoadField('cc_per_client', e.target.value)} />
              </Field>
            </Row>
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

function loadDraft(): TestConfig | null {
  try {
    const raw = localStorage.getItem(DRAFT_KEY)
    return raw ? (JSON.parse(raw) as TestConfig) : null
  } catch {
    return null
  }
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function TestControl() {
  const { savedProfiles, saveProfile, draftConfig, setDraftConfig } = useTestStore()
  const [config, setConfig] = useState<TestConfig>(() => loadDraft() ?? defaultConfig())
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
      localStorage.setItem(DRAFT_KEY, JSON.stringify(config))
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

  const handleAddAssoc = () => {
    const newAssoc = defaultAssociation(config.associations.length)
    setIsNewAssoc(true)
    setEditingAssoc(newAssoc)
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

  const protoLabel = (p: Protocol) => p === 'tcp' ? 'TCP' : p === 'http1' ? 'HTTP/1.1' : 'HTTP/2'
  const modeLabel = (m: NetworkMode) => m === 'loopback' ? 'Loopback' : m === 'namespace' ? 'Namespace' : 'External Port'

  return (
    <>
      <AssociationDialog
        assoc={editingAssoc ?? defaultAssociation(0)}
        open={!!editingAssoc}
        onSave={handleSaveAssoc}
        onCancel={() => setEditingAssoc(null)}
      />

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
                  <option value="cps">CPS — Connections/s</option>
                  <option value="cc">CC — Concurrent Connections</option>
                  <option value="bw">BW — Bandwidth</option>
                </NativeSelect>
              </Field>
              <Field label="Duration" unit="sec (0=manual)">
                <Input type="number" value={config.duration_secs}
                  onChange={(e) => setField('duration_secs', Number(e.target.value))} />
              </Field>
            </Row>
            <Field label="Total Clients" unit="0=각 association별 count 사용">
              <Input type="number" min={0} value={config.total_clients}
                onChange={(e) => setField('total_clients', Number(e.target.value))} />
            </Field>
          </Section>

          {/* Default Load */}
          <Section title="Default Load (per client)" defaultOpen>
            {config.test_type === 'cps' ? (
              <Field label="CPS per Client" unit="/s · 전체 CPS = clients × cps_per_client">
                <Input type="number" value={config.default_load.cps_per_client ?? 100}
                  onChange={(e) => setLoadField('cps_per_client', e.target.value)} />
              </Field>
            ) : (
              <Field label="CC per Client" unit="전체 CC = clients × cc_per_client">
                <Input type="number" value={config.default_load.cc_per_client ?? 50}
                  onChange={(e) => setLoadField('cc_per_client', e.target.value)} />
              </Field>
            )}
            {(estimatedCps !== null || estimatedCc !== null) && (
              <div className="text-xs text-muted-foreground bg-subtle rounded px-3 py-2">
                예상 전체 부하:{' '}
                <span className="text-primary font-bold">
                  {estimatedCps !== null ? `~${estimatedCps.toLocaleString()} CPS` : `~${estimatedCc?.toLocaleString()} CC`}
                </span>
              </div>
            )}
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
            <Field label="Max In-flight per Client" unit="blank=auto">
              <Input type="number" value={config.default_load.max_inflight_per_client ?? ''} placeholder="auto"
                onChange={(e) => setLoadField('max_inflight_per_client', e.target.value)} />
            </Field>
            <Field label="Ramp-up" unit="sec (0=off)">
              <Input type="number" value={config.default_load.ramp_up_secs ?? 0} min={0}
                onChange={(e) => setLoadField('ramp_up_secs', e.target.value)} />
            </Field>
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
            <Field label="Mode">
              <NativeSelect value={config.network.mode}
                onChange={(e) => setNetworkField('mode', e.target.value as NetworkMode)}>
                <option value="loopback">Loopback (localhost, 개발/검증용)</option>
                <option value="namespace">Namespace (Linux netns, CAP_NET_ADMIN 필요)</option>
                <option value="external_port">External Port (Phase 11, 미구현)</option>
              </NativeSelect>
            </Field>
            {config.network.mode === 'namespace' && (
              <Field label="NS Prefix">
                <Input value={config.network.ns.netns_prefix}
                  onChange={(e) => setNetworkField('ns', { ...config.network.ns, netns_prefix: e.target.value })} />
              </Field>
            )}
            {config.network.mode === 'external_port' && (
              <div className="text-xs text-warning bg-warning/10 border border-warning rounded px-3 py-2">
                ⚠ External Port 모드는 Phase 11에서 구현 예정입니다.
              </div>
            )}
            <label className="flex items-center gap-2.5 text-sm cursor-pointer">
              <Switch
                checked={config.network.tcp_quickack}
                onCheckedChange={(v) => setNetworkField('tcp_quickack', v)}
              />
              TCP_QUICKACK (disable delayed ACK)
            </label>
          </Section>

          {/* Associations */}
          <Section title={`Associations (${config.associations.length}) — Mode: ${modeLabel(config.network.mode)}`} defaultOpen>
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-xs">
                <thead>
                  <tr className="text-muted-foreground text-[11px] border-b border-border">
                    <th className="text-left px-2 py-1.5">Name</th>
                    <th className="text-left px-2 py-1.5">Protocol</th>
                    <th className="text-left px-2 py-1.5">Client Net</th>
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
                      <td className="px-2 py-1.5 font-semibold text-primary">
                        {protoLabel(assoc.protocol)}
                        {assoc.tls && <Badge variant="warning" className="ml-1.5">TLS</Badge>}
                      </td>
                      <td className="px-2 py-1.5 text-muted-foreground font-mono">
                        {assoc.client_net.base_ip}{assoc.client_net.count ? `×${assoc.client_net.count}` : '×auto'}
                      </td>
                      <td className="px-2 py-1.5 text-muted-foreground">
                        {assoc.server.id} — {assoc.server.ip ?? '0.0.0.0'}:{assoc.server.port}
                      </td>
                      <td className="px-2 py-1.5 text-muted-foreground/50">
                        {assoc.load ? 'custom' : 'default'}
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
            <Button variant="secondary" size="sm" onClick={handleAddAssoc} className="self-start">
              <Plus className="h-3.5 w-3.5" />
              Add Association
            </Button>
          </Section>

        </CardContent>
      </Card>
    </>
  )
}
