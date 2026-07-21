import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTimeoutPoll } from '@vueuse/core'
import { createProviderAdapter } from '@/data/tauri'
import type { AgentSession, DashboardSnapshot, ProviderId, UiText } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { localeTag, translateUiText } from '@/i18n/format'

const POLLING_INTERVAL_MS = 15_000
const QUOTA_POLLING_INTERVAL_MS = 120_000

// Retain the last known quotas and their warning while a lightweight session poll is running.
function mergeDashboardSnapshot(
  previous: DashboardSnapshot | null,
  next: DashboardSnapshot,
  includeQuotas: boolean,
  quotaErrors: ReadonlyMap<ProviderId, UiText>,
): DashboardSnapshot {
  if (!previous) return next

  const nextQuotaProviders = new Set(next.quotas.map((quota) => quota.provider))
  const preservedQuotas = previous.quotas.filter((quota) => !nextQuotaProviders.has(quota.provider))
  const providers = includeQuotas
    ? next.providers
    : next.providers.map((provider) => {
        const quotaError = quotaErrors.get(provider.id)
        return !provider.error && quotaError
          ? { ...provider, error: quotaError }
          : provider
      })

  return {
    ...next,
    providers,
    quotas: [...next.quotas, ...preservedQuotas],
  }
}

export function useDashboard() {
  const adapter = createProviderAdapter()
  const { t, locale } = useI18n({ useScope: 'global' })
  const snapshot = ref<DashboardSnapshot | null>(null)
  const activeProvider = ref<ProviderId | 'all'>('all')
  const selectedSessionId = ref('')
  const search = ref('')
  const refreshing = ref(false)
  const error = ref<string | null>(null)
  let pendingLoad: Promise<void> | null = null
  let pendingLoadIncludesQuotas = false
  let queuedQuotaLoad: Promise<void> | null = null
  const quotaErrors = new Map<ProviderId, UiText>()

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

  const selectedSession = computed<AgentSession | null>(
    () => sessions.value.find((session) => session.id === selectedSessionId.value) ?? null,
  )

  watch(
    sessions,
    (visibleSessions) => {
      const selectedIsVisible = visibleSessions.some((session) => session.id === selectedSessionId.value)
      if (!selectedIsVisible) selectedSessionId.value = visibleSessions[0]?.id ?? ''
    },
    { flush: 'sync', immediate: true },
  )

  const activeCount = computed(
    () => snapshot.value?.sessions.filter((session) => session.status === 'working').length ?? 0,
  )

  const attentionCount = computed(
    () =>
      snapshot.value?.sessions.filter((session) => session.status === 'waiting' || session.status === 'attention')
        .length ?? 0,
  )

  // Merge lightweight session refreshes without discarding the slower quota state.
  async function performLoad(includeQuotas: boolean) {
    error.value = null
    try {
      const nextSnapshot = await adapter.getSnapshot({ includeQuotas })
      if (includeQuotas) {
        const providersWithQuota = new Set(nextSnapshot.quotas.map((quota) => quota.provider))
        nextSnapshot.providers.forEach((provider) => {
          if (providersWithQuota.has(provider.id)) {
            quotaErrors.delete(provider.id)
          } else if (provider.error) {
            quotaErrors.set(provider.id, provider.error)
          } else {
            quotaErrors.delete(provider.id)
          }
        })
      }
      snapshot.value = mergeDashboardSnapshot(snapshot.value, nextSnapshot, includeQuotas, quotaErrors)
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : String(reason)
    }
  }

  function startLoad(includeQuotas: boolean): Promise<void> {
    pendingLoadIncludesQuotas = includeQuotas
    const request = performLoad(includeQuotas).finally(() => {
      if (pendingLoad === request) {
        pendingLoad = null
        pendingLoadIncludesQuotas = false
      }
    })
    pendingLoad = request
    return request
  }

  // Coalesce overlapping polls and promote a queued request when fresh quotas are required.
  function load(includeQuotas: boolean): Promise<void> {
    if (!pendingLoad) return startLoad(includeQuotas)
    if (!includeQuotas || pendingLoadIncludesQuotas) return pendingLoad

    if (!queuedQuotaLoad) {
      queuedQuotaLoad = pendingLoad
        .then(() => load(true))
        .finally(() => {
          queuedQuotaLoad = null
        })
    }
    return queuedQuotaLoad
  }

  async function refresh() {
    refreshing.value = true
    try {
      await load(true)
    } finally {
      refreshing.value = false
    }
  }

  function selectProvider(provider: ProviderId | 'all') {
    activeProvider.value = provider
  }

  function selectSession(id: string) {
    selectedSessionId.value = id
  }

  const { resume: resumeSessionPolling } = useTimeoutPoll(() => load(false), POLLING_INTERVAL_MS, {
    immediate: false,
  })

  const { resume: resumeQuotaPolling } = useTimeoutPoll(() => load(true), QUOTA_POLLING_INTERVAL_MS, {
    immediate: false,
  })

  onMounted(async () => {
    await load(true)
    resumeSessionPolling()
    resumeQuotaPolling()
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
    quotaPollingIntervalSeconds: QUOTA_POLLING_INTERVAL_MS / 1_000,
    error,
    refresh,
    selectProvider,
    selectSession,
  }
}
