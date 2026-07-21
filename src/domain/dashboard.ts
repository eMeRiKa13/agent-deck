export type ProviderId = 'codex' | 'claude' | 'cursor'
export type SessionStatus = 'working' | 'waiting' | 'completed' | 'attention'

export interface UiMessage {
  key: string
  params?: Record<string, string | number>
}

export type UiText = string | UiMessage

export interface ProviderDefinition {
  id: ProviderId
  name: string
  shortName: string
  accent: string
  connected: boolean
  error?: UiText | null
}

export interface SessionEvent {
  id: string
  kind: 'user' | 'assistant' | 'tool' | 'system'
  title: UiText
  body: UiText
  timestampMs: number
}

export interface AgentSession {
  id: string
  provider: ProviderId
  title: UiText
  project: UiText
  branch: string
  model: string
  status: SessionStatus
  startedAtMs: number
  updatedAtMs: number
  tokenCount: number
  summary: UiText
  events: SessionEvent[]
}

export interface QuotaWindow {
  id: string
  label: UiText
  usedPercent: number
  resetAtMs?: number | null
  resetText?: UiText | null
}

export type UsageCost =
  | { kind: 'credits'; value: number }
  | { kind: 'extraUsage'; value: number; currency: string }
  | { kind: 'includedOnDemand'; included: number; onDemand: number }
  | { kind: 'localMeasurement'; measuredAtMs: number }
  | { kind: 'tokensToday'; value: number }
  | { kind: 'consumption'; value: number }

export interface ProviderQuota {
  provider: ProviderId
  plan: string
  account: UiText
  windows: QuotaWindow[]
  cost?: UsageCost | null
}

export interface DashboardSnapshot {
  providers: ProviderDefinition[]
  sessions: AgentSession[]
  quotas: ProviderQuota[]
  refreshedAtMs: number
}

export interface DashboardSnapshotOptions {
  includeQuotas?: boolean
}

export interface ProviderAdapter {
  getSnapshot(options?: DashboardSnapshotOptions): Promise<DashboardSnapshot>
}
