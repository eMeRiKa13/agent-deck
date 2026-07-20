export type ProviderId = 'codex' | 'claude' | 'cursor'
export type SessionStatus = 'working' | 'waiting' | 'completed' | 'attention'

export interface ProviderDefinition {
  id: ProviderId
  name: string
  shortName: string
  accent: string
  connected: boolean
  error?: string | null
}

export interface SessionEvent {
  id: string
  kind: 'user' | 'assistant' | 'tool' | 'system'
  title: string
  body: string
  time: string
}

export interface AgentSession {
  id: string
  provider: ProviderId
  title: string
  project: string
  branch: string
  model: string
  status: SessionStatus
  elapsed: string
  updatedAt: string
  tokenCount: number
  summary: string
  events: SessionEvent[]
}

export interface QuotaWindow {
  id: string
  label: string
  usedPercent: number
  resetLabel: string
}

export interface ProviderQuota {
  provider: ProviderId
  plan: string
  account: string
  windows: QuotaWindow[]
  costLabel?: string
}

export interface DashboardSnapshot {
  providers: ProviderDefinition[]
  sessions: AgentSession[]
  quotas: ProviderQuota[]
  refreshedAt: string
}

export interface ProviderAdapter {
  getSnapshot(): Promise<DashboardSnapshot>
}
