<script setup lang="ts">
import {
  BotIcon,
  BracesIcon,
  Clock3Icon,
  GaugeIcon,
  GitBranchIcon,
  ListTreeIcon,
  WrenchIcon,
  XIcon,
} from '@lucide/vue'
import { computed, nextTick, onBeforeUnmount, useTemplateRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import ProviderMark from '@/components/ProviderMark.vue'
import SessionActivityGroup from '@/components/SessionActivityGroup.vue'
import SessionTokenUsage from '@/components/SessionTokenUsage.vue'
import StatusPill from '@/components/StatusPill.vue'
import type { AgentSession, SessionStatus } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import {
  formatCompactNumber,
  formatDuration,
  formatMilliseconds,
  formatRelativeTime,
  translateUiText,
} from '@/i18n/format'

const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)

const { session, compact = false, open = false } = defineProps<{
  session: AgentSession
  compact?: boolean
  open?: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const providerNames = {
  codex: 'Codex',
  claude: 'Claude',
  cursor: 'Cursor',
} as const

const statusMessages: Record<SessionStatus, { title: string, body: string }> = {
  working: { title: 'detail.workingTitle', body: 'detail.workingBody' },
  waiting: { title: 'detail.waitingTitle', body: 'detail.waitingBody' },
  completed: { title: 'detail.completedTitle', body: 'detail.completedBody' },
  attention: { title: 'detail.attentionTitle', body: 'detail.attentionBody' },
}

const statusMessage = computed(() => statusMessages[session.status])
const translatedTitle = computed(() => translateUiText(session.title, t))
const translatedSummary = computed(() => translateUiText(session.summary, t))
const showSummary = computed(() => {
  const title = translatedTitle.value.trim().toLocaleLowerCase(currentLocale.value)
  const summary = translatedSummary.value.trim().toLocaleLowerCase(currentLocale.value)
  return summary.length > 0 && summary !== title
})
const activityCount = computed(() => session.activityGroups.reduce(
  (count, group) => count + group.items.length,
  0,
))
const recentActivityGroups = computed(() => [...session.activityGroups].reverse())
const subagents = computed(() => session.providerDetails.provider === 'codex'
  ? session.providerDetails.subagents
  : [])

const metrics = computed(() => {
  const values = [
    {
      icon: Clock3Icon,
      label: t('detail.duration'),
      value: formatDuration(session.startedAtMs, session.updatedAtMs, t),
    },
    {
      icon: ListTreeIcon,
      label: t('detail.turns'),
      value: session.metrics.turnCount === undefined
        ? t('detail.unavailableShort')
        : formatCompactNumber(session.metrics.turnCount, currentLocale.value),
    },
    {
      icon: WrenchIcon,
      label: t('detail.tools'),
      value: session.metrics.toolCallCount === undefined
        ? t('detail.unavailableShort')
        : formatCompactNumber(session.metrics.toolCallCount, currentLocale.value),
    },
  ]

  if (session.tokenUsage) {
    values.push({
      icon: BracesIcon,
      label: t('detail.tokensTotal'),
      value: formatCompactNumber(session.tokenUsage.total, currentLocale.value),
    })
  } else if (session.metrics.contextUsagePercent !== undefined) {
    values.push({
      icon: GaugeIcon,
      label: t('detail.context'),
      value: `${Math.round(session.metrics.contextUsagePercent)} %`,
    })
  }

  return values
})

const providerFacts = computed(() => {
  const details = session.providerDetails
  const facts: Array<{ label: string, value: string }> = []

  if (details.provider === 'codex') {
    if (details.reasoningEffort) facts.push({ label: t('detail.reasoningEffort'), value: details.reasoningEffort })
    if (details.collaborationMode) facts.push({ label: t('detail.collaborationMode'), value: details.collaborationMode })
  } else if (details.provider === 'claude') {
    if (details.effort) facts.push({ label: t('detail.reasoningEffort'), value: details.effort })
    if (details.mode) facts.push({ label: t('detail.mode'), value: details.mode })
    if (details.completedTurns !== undefined) {
      facts.push({ label: t('detail.completedTurns'), value: String(details.completedTurns) })
    }
  } else {
    if (details.forceMode) facts.push({ label: t('detail.cursorForceMode'), value: details.forceMode })
    if (details.unifiedMode) facts.push({ label: t('detail.mode'), value: details.unifiedMode })
  }

  if (session.metrics.contextWindowTokens !== undefined) {
    facts.push({
      label: t('detail.contextWindow'),
      value: formatCompactNumber(session.metrics.contextWindowTokens, currentLocale.value),
    })
  }
  if (session.metrics.lastTurnDurationMs !== undefined) {
    facts.push({
      label: t('detail.lastTurnDuration'),
      value: formatMilliseconds(session.metrics.lastTurnDurationMs, currentLocale.value),
    })
  }
  if (session.metrics.filesChanged !== undefined) {
    facts.push({ label: t('detail.filesChanged'), value: String(session.metrics.filesChanged) })
  }
  if (session.metrics.linesAdded !== undefined || session.metrics.linesRemoved !== undefined) {
    facts.push({
      label: t('detail.lineChanges'),
      value: `+${session.metrics.linesAdded ?? 0} / −${session.metrics.linesRemoved ?? 0}`,
    })
  }
  if (session.metrics.thinkingDurationMs !== undefined) {
    facts.push({
      label: t('detail.thinkingDuration'),
      value: formatMilliseconds(session.metrics.thinkingDurationMs, currentLocale.value),
    })
  }

  return facts
})

const panel = useTemplateRef<HTMLElement>('panel')
const closeButton = useTemplateRef<HTMLButtonElement>('closeButton')
let previousActiveElement: HTMLElement | null = null

const focusableSelector = [
  'button:not([disabled])',
  'summary',
  '[href]',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

watch(
  () => compact && open,
  async (isOpen) => {
    if (isOpen) {
      previousActiveElement = document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null
      await nextTick()
      closeButton.value?.focus()
      return
    }

    const elementToRestore = previousActiveElement
    previousActiveElement = null
    await nextTick()
    if (elementToRestore?.isConnected) elementToRestore.focus()
  },
  { flush: 'sync', immediate: true },
)

onBeforeUnmount(() => {
  if (previousActiveElement?.isConnected) previousActiveElement.focus()
})

function handleKeydown(event: KeyboardEvent) {
  if (!compact || !open) return

  if (event.key === 'Escape') {
    event.preventDefault()
    emit('close')
    return
  }

  if (event.key !== 'Tab' || !panel.value) return

  const focusable = [...panel.value.querySelectorAll<HTMLElement>(focusableSelector)]
    .filter((element) => !element.hasAttribute('disabled'))
  if (!focusable.length) return

  const first = focusable[0]
  const last = focusable[focusable.length - 1]

  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last?.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first?.focus()
  }
}
</script>

<template>
  <section
    ref="panel"
    class="conversation-panel"
    :aria-label="t('detail.aria')"
    :role="compact && open ? 'dialog' : undefined"
    :aria-modal="compact && open ? 'true' : undefined"
    @keydown="handleKeydown"
  >
    <div v-if="compact" data-tauri-drag-region class="conversation-panel__drag-strip" />

    <header class="conversation-header">
      <div class="conversation-header__identity">
        <ProviderMark :provider="session.provider" size="lg" />
        <div class="conversation-header__copy">
          <div class="conversation-header__provider">
            <span>{{ providerNames[session.provider] }}</span>
            <StatusPill :status="session.status" />
          </div>
          <h1>{{ translatedTitle }}</h1>
          <div class="conversation-header__meta">
            <span>{{ translateUiText(session.project, t) }}</span>
            <span v-if="session.branch"><GitBranchIcon :size="14" />{{ session.branch }}</span>
            <span v-if="session.model">{{ session.model }}</span>
          </div>
        </div>
      </div>
      <button
        v-if="compact"
        ref="closeButton"
        type="button"
        class="conversation-panel__close"
        :aria-label="t('detail.close')"
        @click="emit('close')"
      >
        <XIcon :size="22" />
      </button>
    </header>

    <div class="conversation-body">
      <section class="session-overview" :aria-label="t('detail.overview')">
        <p v-if="showSummary" class="session-overview__summary">{{ translatedSummary }}</p>
        <div class="session-overview__status">
          <span>{{ t(statusMessage.title) }}</span>
          <span>{{ t(statusMessage.body) }}</span>
          <span>·</span>
          <span>{{ t('detail.updated', { time: formatRelativeTime(session.updatedAtMs, currentLocale, t) }) }}</span>
        </div>
        <div class="session-metrics">
          <div v-for="metric in metrics" :key="metric.label" class="session-metric">
            <component :is="metric.icon" :size="18" aria-hidden="true" />
            <div>
              <strong>{{ metric.value }}</strong>
              <span>{{ metric.label }}</span>
            </div>
          </div>
        </div>
      </section>

      <details v-if="providerFacts.length" class="session-detail-section">
        <summary>{{ t('detail.providerDetails', { provider: providerNames[session.provider] }) }}</summary>
        <dl class="session-facts">
          <div v-for="fact in providerFacts" :key="fact.label">
            <dt>{{ fact.label }}</dt>
            <dd>{{ fact.value }}</dd>
          </div>
        </dl>
      </details>

      <details class="session-detail-section">
        <summary>{{ t('detail.tokenDetails') }}</summary>
        <SessionTokenUsage :usage="session.tokenUsage" />
      </details>

      <details v-if="subagents.length" class="session-detail-section">
        <summary>{{ t('detail.subagents', { count: subagents.length }) }}</summary>
        <div class="subagent-list">
          <article v-for="subagent in subagents" :key="subagent.id" class="subagent-row">
            <BotIcon :size="18" aria-hidden="true" />
            <div>
              <strong>{{ subagent.name }}</strong>
              <span>{{ [subagent.role, subagent.model, subagent.reasoningEffort].filter(Boolean).join(' · ') }}</span>
            </div>
            <StatusPill :status="subagent.status" />
          </article>
        </div>
      </details>

      <section class="session-activity" :aria-label="t('detail.activity')">
        <header class="session-activity__header">
          <div>
            <span>{{ t('detail.activity') }}</span>
            <strong>{{ t('detail.activityCount', { count: activityCount }, activityCount) }}</strong>
          </div>
          <span v-if="session.activityTruncated">{{ t('detail.activityTruncated') }}</span>
        </header>

        <div v-if="recentActivityGroups.length" class="activity-groups">
          <SessionActivityGroup
            v-for="(group, index) in recentActivityGroups"
            :key="group.id"
            :group="group"
            :index="session.activityGroups.length - index - 1"
          />
        </div>
        <p v-else class="session-activity__empty">{{ t('detail.noActivity') }}</p>
      </section>
    </div>
  </section>
</template>
