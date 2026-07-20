import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTimeoutPoll } from '@vueuse/core'
import { createProviderAdapter } from '@/data/tauri'
import type { AgentSession, DashboardSnapshot, ProviderId } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { localeTag, translateUiText } from '@/i18n/format'

const adapter = createProviderAdapter()
const POLLING_INTERVAL_MS = 15_000

export function useDashboard() {
  const { t, locale } = useI18n({ useScope: 'global' })
  const snapshot = ref<DashboardSnapshot | null>(null)
  const activeProvider = ref<ProviderId | 'all'>('all')
  const selectedSessionId = ref('codex-dashboard')
  const search = ref('')
  const refreshing = ref(false)
  const error = ref<string | null>(null)
  let pendingLoad: Promise<void> | null = null

  const sessions = computed(() => {
    const tag = localeTag(locale.value as SupportedLocale)
    const query = search.value.trim().toLocaleLowerCase(tag)
    return (snapshot.value?.sessions ?? []).filter((session) => {
      const matchesProvider = activeProvider.value === 'all' || session.provider === activeProvider.value
      const matchesSearch =
        query.length === 0 ||
        `${translateUiText(session.title, t)} ${translateUiText(session.project, t)} ${translateUiText(session.summary, t)}`
          .toLocaleLowerCase(tag)
          .includes(query)
      return matchesProvider && matchesSearch
    })
  })

  const selectedSession = computed<AgentSession | null>(() => {
    const allSessions = snapshot.value?.sessions ?? []
    return allSessions.find((session) => session.id === selectedSessionId.value) ?? sessions.value[0] ?? null
  })

  const activeCount = computed(
    () => snapshot.value?.sessions.filter((session) => session.status === 'working').length ?? 0,
  )

  const attentionCount = computed(
    () =>
      snapshot.value?.sessions.filter((session) => session.status === 'waiting' || session.status === 'attention')
        .length ?? 0,
  )

  async function performLoad() {
    error.value = null
    try {
      snapshot.value = await adapter.getSnapshot()
      const selectedExists = snapshot.value.sessions.some((session) => session.id === selectedSessionId.value)
      if (!selectedExists && snapshot.value.sessions[0]) {
        selectedSessionId.value = snapshot.value.sessions[0].id
      }
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : String(reason)
    }
  }

  function load() {
    if (pendingLoad) return pendingLoad
    pendingLoad = performLoad().finally(() => {
      pendingLoad = null
    })
    return pendingLoad
  }

  async function refresh() {
    refreshing.value = true
    try {
      await load()
    } finally {
      refreshing.value = false
    }
  }

  function selectProvider(provider: ProviderId | 'all') {
    activeProvider.value = provider
    const firstMatch = sessions.value[0]
    if (firstMatch) selectedSessionId.value = firstMatch.id
  }

  function selectSession(id: string) {
    selectedSessionId.value = id
  }

  const { resume: resumePolling } = useTimeoutPoll(load, POLLING_INTERVAL_MS, {
    immediate: false,
  })

  onMounted(async () => {
    await load()
    resumePolling()
  })

  return {
    snapshot,
    sessions,
    selectedSession,
    selectedSessionId,
    activeProvider,
    activeCount,
    attentionCount,
    search,
    refreshing,
    pollingIntervalSeconds: POLLING_INTERVAL_MS / 1_000,
    error,
    refresh,
    selectProvider,
    selectSession,
  }
}
