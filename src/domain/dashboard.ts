// Shared dashboard contract mirrored by the Rust serialization model in src-tauri/src/domain.rs.
export type ProviderId = 'codex' | 'claude' | 'cursor'
export type SessionStatus = 'working' | 'waiting' | 'completed' | 'attention'

export interface UiMessage {
  key: string
  params?: Record<string, string | number>
}

export type UiText = string | UiMessage

export type ActivityKind = 'user' | 'assistant' | 'tool' | 'system' | 'subagent'
export type ActivityStatus = 'running' | 'completed' | 'error' | 'cancelled'
export type TokenUsageQuality = 'detailed' | 'totalOnly'

export interface ProviderDefinition {
  id: ProviderId
  name: string
  shortName: string
  accent: string
  connected: boolean
  error?: UiText | null
}

export interface TokenUsage {
  quality: TokenUsageQuality
  total: number
  input?: number
  output?: number
  cachedInput?: number
  cacheWriteInput?: number
  reasoningOutput?: number
}

export interface SessionMetrics {
  turnCount?: number
  toolCallCount?: number
  toolErrorCount?: number
  contextWindowTokens?: number
  contextUsagePercent?: number
  lastTurnDurationMs?: number
  thinkingDurationMs?: number
  filesChanged?: number
  linesAdded?: number
  linesRemoved?: number
}

export interface SubagentSummary {
  id: string
  name: string
  path?: string
  role?: string
  model?: string
  reasoningEffort?: string
  status: SessionStatus
  updatedAtMs: number
  tokenUsage?: TokenUsage
}

export type ProviderSessionDetails =
  | {
      provider: 'codex'
      reasoningEffort?: string
      collaborationMode?: string
      subagents: SubagentSummary[]
    }
  | {
      provider: 'claude'
      effort?: string
      mode?: string
      completedTurns?: number
    }
  | {
      provider: 'cursor'
      forceMode?: string
      unifiedMode?: string
    }

export interface ActivityItem {
  id: string
  kind: ActivityKind
  title: UiText
  body: UiText
  timestampMs: number
  status?: ActivityStatus
  durationMs?: number
  count?: number
}

export interface ActivityGroup {
  id: string
  startedAtMs: number
  completedAtMs?: number
  status?: ActivityStatus
  items: ActivityItem[]
}

export interface AgentSession {
  id: string
  provider: ProviderId
  title: UiText
  project: UiText
  branch: string
  model?: string
  status: SessionStatus
  startedAtMs: number
  updatedAtMs: number
  tokenUsage?: TokenUsage
  metrics: SessionMetrics
  providerDetails: ProviderSessionDetails
  summary: UiText
  activityGroups: ActivityGroup[]
  activityTruncated: boolean
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
