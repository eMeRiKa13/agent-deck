import type { DashboardSnapshot, DashboardSnapshotOptions, ProviderAdapter, UiMessage } from '@/domain/dashboard'

const ui = (key: string, params?: UiMessage['params']): UiMessage => ({ key, ...(params ? { params } : {}) })
const minute = 60_000
const hour = 60 * minute
const day = 24 * hour

function createSnapshot(): DashboardSnapshot {
  const now = Date.now()

  return {
    refreshedAtMs: now,
    providers: [
      { id: 'codex', name: 'Codex', shortName: 'CX', accent: '#111111', connected: true },
      { id: 'claude', name: 'Claude', shortName: 'CL', accent: '#d97757', connected: true },
      { id: 'cursor', name: 'Cursor', shortName: 'CU', accent: '#6d5dfc', connected: true },
    ],
    sessions: [
      {
        id: 'codex-dashboard', provider: 'codex', project: 'agent-deck', branch: 'main', model: 'GPT-5.6 Sol',
        title: ui('demo.codexDashboard.title'), summary: ui('demo.codexDashboard.summary'), status: 'working',
        startedAtMs: now - 12 * minute, updatedAtMs: now, tokenCount: 18_340,
        events: [
          { id: 'c1', kind: 'user', title: ui('demo.codexDashboard.userTitle'), body: ui('demo.codexDashboard.userBody'), timestampMs: now - 12 * minute },
          { id: 'c2', kind: 'assistant', title: ui('demo.codexDashboard.planTitle'), body: ui('demo.codexDashboard.planBody'), timestampMs: now - 11 * minute },
          { id: 'c3', kind: 'tool', title: ui('demo.codexDashboard.toolTitle'), body: ui('demo.codexDashboard.toolBody'), timestampMs: now - 8 * minute },
          { id: 'c4', kind: 'system', title: ui('demo.codexDashboard.systemTitle'), body: ui('demo.codexDashboard.systemBody'), timestampMs: now },
        ],
      },
      {
        id: 'claude-sync', provider: 'claude', project: 'liblio', branch: 'feat/sync-health', model: 'Claude Opus 4.7',
        title: ui('demo.claudeSync.title'), summary: ui('demo.claudeSync.summary'), status: 'waiting',
        startedAtMs: now - 33 * minute, updatedAtMs: now - 2 * minute, tokenCount: 42_110,
        events: [
          { id: 'cl1', kind: 'user', title: ui('demo.claudeSync.userTitle'), body: ui('demo.claudeSync.userBody'), timestampMs: now - 33 * minute },
          { id: 'cl2', kind: 'tool', title: ui('demo.claudeSync.toolTitle'), body: ui('demo.claudeSync.toolBody'), timestampMs: now - 24 * minute },
          { id: 'cl3', kind: 'assistant', title: ui('demo.claudeSync.answerTitle'), body: ui('demo.claudeSync.answerBody'), timestampMs: now - 2 * minute },
        ],
      },
      {
        id: 'cursor-index', provider: 'cursor', project: 'atlas-web', branch: 'fix/chat-index', model: 'Auto',
        title: ui('demo.cursorIndex.title'), summary: ui('demo.cursorIndex.summary'), status: 'attention',
        startedAtMs: now - 12 * minute, updatedAtMs: now - 4 * minute, tokenCount: 12_860,
        events: [
          { id: 'cu1', kind: 'user', title: ui('demo.cursorIndex.userTitle'), body: ui('demo.cursorIndex.userBody'), timestampMs: now - 10 * minute },
          { id: 'cu2', kind: 'tool', title: ui('demo.cursorIndex.toolTitle'), body: ui('demo.cursorIndex.toolBody'), timestampMs: now - 5 * minute },
          { id: 'cu3', kind: 'system', title: ui('demo.cursorIndex.systemTitle'), body: ui('demo.cursorIndex.systemBody'), timestampMs: now - 4 * minute },
        ],
      },
      {
        id: 'codex-review', provider: 'codex', project: 'platform-api', branch: 'chore/rate-limit', model: 'GPT-5.6 Terra',
        title: ui('demo.codexReview.title'), summary: ui('demo.codexReview.summary'), status: 'completed',
        startedAtMs: now - 42 * minute, updatedAtMs: now - 18 * minute, tokenCount: 29_440,
        events: [{ id: 'cr1', kind: 'assistant', title: ui('demo.codexReview.eventTitle'), body: ui('demo.codexReview.eventBody'), timestampMs: now - 18 * minute }],
      },
      {
        id: 'claude-tests', provider: 'claude', project: 'commerce', branch: 'test/checkout', model: 'Claude Sonnet 4.6',
        title: ui('demo.claudeTests.title'), summary: ui('demo.claudeTests.summary'), status: 'working',
        startedAtMs: now - 7 * minute, updatedAtMs: now - minute, tokenCount: 9_480,
        events: [{ id: 'ct1', kind: 'tool', title: ui('demo.claudeTests.eventTitle'), body: ui('demo.claudeTests.eventBody'), timestampMs: now - minute }],
      },
      {
        id: 'cursor-copy', provider: 'cursor', project: 'mobile-app', branch: 'copy/onboarding', model: 'Composer 2',
        title: ui('demo.cursorCopy.title'), summary: ui('demo.cursorCopy.summary'), status: 'completed',
        startedAtMs: now - 65 * minute, updatedAtMs: now - 46 * minute, tokenCount: 16_320,
        events: [{ id: 'cc1', kind: 'assistant', title: ui('demo.cursorCopy.eventTitle'), body: ui('demo.cursorCopy.eventBody'), timestampMs: now - 46 * minute }],
      },
    ],
    quotas: [
      {
        provider: 'codex', plan: 'Pro', account: ui('generated.primaryAccount'),
        windows: [{ id: 'codex-global', label: ui('quota.global'), usedPercent: 39, resetAtMs: now + 4 * day }],
        cost: { kind: 'tokensToday', value: 183 },
      },
      {
        provider: 'claude', plan: 'Max 20×', account: ui('generated.primaryAccount'),
        windows: [
          { id: 'claude-session', label: ui('quota.fiveHours'), usedPercent: 74, resetAtMs: now + 48 * minute },
          { id: 'claude-week', label: ui('quota.weekly'), usedPercent: 46, resetAtMs: now + 5 * day },
        ],
        cost: { kind: 'tokensToday', value: 256 },
      },
      {
        provider: 'cursor', plan: 'Pro', account: ui('generated.personalWorkspace'),
        windows: [
          { id: 'cursor-plan', label: ui('quota.included'), usedPercent: 57, resetAtMs: now + 12 * day },
          { id: 'cursor-auto', label: ui('quota.autoModel'), usedPercent: 31, resetText: ui('quota.billingCycle') },
        ],
        cost: { kind: 'consumption', value: 8.42 },
      },
    ],
  }
}

export class DemoProviderAdapter implements ProviderAdapter {
  async getSnapshot(options?: DashboardSnapshotOptions): Promise<DashboardSnapshot> {
    const snapshot = createSnapshot()
    return options?.includeQuotas === false
      ? { ...snapshot, quotas: [] }
      : snapshot
  }
}
