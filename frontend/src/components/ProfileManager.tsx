import { useState } from 'react'
import { Trash2, ChevronDown, ChevronRight, ArrowUpRight } from 'lucide-react'
import { TestConfig } from '../api/client'
import { useTestStore } from '../store/testStore'
import { Card, CardContent } from './ui/card'
import { Button } from './ui/button'
import { Badge } from './ui/badge'
import { cn } from '@/lib/utils'

function configSummary(c: TestConfig): string {
  const protos = [...new Set(c.associations.map((a) => a.protocol.toUpperCase()))].join('/')
  return `${c.test_type.toUpperCase()} · ${protos} · ${c.associations.length} association(s) · ${c.duration_secs}s`
}

const testTypeBadge = (t: string): Parameters<typeof Badge>[0]['variant'] =>
  t === 'cps' ? 'cps' : t === 'bw' ? 'bw' : 'cc'

export default function ProfileManager({ onLoadConfig }: { onLoadConfig?: (c: TestConfig) => void }) {
  const { savedProfiles, deleteProfile } = useTestStore()
  const [expanded, setExpanded] = useState<string | null>(null)

  return (
    <div>
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-base font-semibold">Saved Configs</h2>
        <span className="text-xs text-muted-foreground">
          Config 탭에서 "Save to Profiles"로 저장합니다.
        </span>
      </div>

      {savedProfiles.length === 0 ? (
        <Card>
          <CardContent className="text-muted-foreground text-sm text-center py-8">
            저장된 설정이 없습니다. Config 탭에서 "Save to Profiles"를 눌러 저장하세요.
          </CardContent>
        </Card>
      ) : (
        <div className="flex flex-col gap-2">
          {savedProfiles.map((c) => {
            const isOpen = expanded === c.id
            return (
              <Card key={c.id}>
                <CardContent className="flex flex-col gap-0 p-0">
                  <div className="flex items-center gap-3 px-4 py-3">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-0.5">
                        <span className="font-semibold text-sm">{c.name}</span>
                        <Badge variant={testTypeBadge(c.test_type)}>{c.test_type.toUpperCase()}</Badge>
                      </div>
                      <div className="text-xs text-muted-foreground">{configSummary(c)}</div>
                    </div>
                    <Button
                      variant="ghost"
                      size="xs"
                      onClick={() => setExpanded(isOpen ? null : c.id)}
                      className="text-muted-foreground"
                    >
                      {isOpen ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
                      {isOpen ? 'Hide' : 'Details'}
                    </Button>
                    {onLoadConfig && (
                      <Button size="xs" onClick={() => onLoadConfig(c)}>
                        <ArrowUpRight className="h-3.5 w-3.5" />
                        Load
                      </Button>
                    )}
                    <Button variant="destructive" size="xs" onClick={() => deleteProfile(c.id)}>
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>

                  {isOpen && (
                    <div className={cn('border-t border-border px-4 pb-4 pt-3')}>
                      <div className="text-[11px] font-bold uppercase tracking-wider text-muted-foreground mb-2">Associations</div>
                      <table className="w-full border-collapse text-xs">
                        <thead>
                          <tr className="text-muted-foreground text-[11px] border-b border-border">
                            <th className="text-left py-1 px-2">Protocol</th>
                            <th className="text-left py-1 px-2">Client</th>
                            <th className="text-left py-1 px-2">Server</th>
                            <th className="text-left py-1 px-2">Load</th>
                          </tr>
                        </thead>
                        <tbody>
                          {c.associations.map((assoc) => (
                            <tr key={assoc.id} className="border-b border-border/50 last:border-0">
                              <td className="py-1.5 px-2 text-primary font-semibold">{assoc.protocol.toUpperCase()}</td>
                              <td className="py-1.5 px-2 text-muted-foreground font-mono">
                                {assoc.client_net.base_ip}/{assoc.client_net.prefix_len ?? 24}
                              </td>
                              <td className="py-1.5 px-2 text-muted-foreground">
                                {assoc.server.ip ?? '0.0.0.0'}:{assoc.server.port}
                              </td>
                              <td className="py-1.5 px-2 text-muted-foreground/60">
                                {assoc.load ? 'custom' : 'default'}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>

                      <details className="mt-3">
                        <summary className="text-xs text-muted-foreground cursor-pointer hover:text-foreground transition-colors">
                          Raw JSON
                        </summary>
                        <pre className="text-[10px] text-muted-foreground bg-subtle rounded px-3 py-2 overflow-auto max-h-48 mt-2 font-mono">
                          {JSON.stringify(c, null, 2)}
                        </pre>
                      </details>
                    </div>
                  )}
                </CardContent>
              </Card>
            )
          })}
        </div>
      )}
    </div>
  )
}
