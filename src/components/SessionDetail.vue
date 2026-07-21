<script setup lang="ts">
import {
  ArrowUpRightIcon,
  CheckIcon,
  Clock3Icon,
  GitBranchIcon,
  RotateCcwIcon,
  TerminalSquareIcon,
  XIcon,
} from '@lucide/vue'
import { computed, nextTick, onBeforeUnmount, useTemplateRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import ProviderMark from '@/components/ProviderMark.vue'
import StatusPill from '@/components/StatusPill.vue'
import type { AgentSession, SessionEvent, SessionStatus } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { formatDuration, formatEventTime, translateUiText } from '@/i18n/format'

const { t, n, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)

const { session, compact = false, open = false } = defineProps<{
  session: AgentSession
  compact?: boolean
  open?: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const eventIcons: Record<SessionEvent['kind'], typeof CheckIcon> = {
  user: ArrowUpRightIcon,
  assistant: CheckIcon,
  tool: TerminalSquareIcon,
  system: RotateCcwIcon,
}

const statusMessages: Record<SessionStatus, { title: string, body: string }> = {
  working: { title: 'detail.workingTitle', body: 'detail.workingBody' },
  waiting: { title: 'detail.waitingTitle', body: 'detail.waitingBody' },
  completed: { title: 'detail.completedTitle', body: 'detail.completedBody' },
  attention: { title: 'detail.attentionTitle', body: 'detail.attentionBody' },
}

const statusMessage = computed(() => statusMessages[session.status])
const panel = useTemplateRef<HTMLElement>('panel')
const closeButton = useTemplateRef<HTMLButtonElement>('closeButton')
let previousActiveElement: HTMLElement | null = null

const focusableSelector = [
  'button:not([disabled])',
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
  { flush: 'sync' },
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
    <header class="conversation-header">
      <div class="conversation-header__identity">
        <ProviderMark :provider="session.provider" size="lg" />
        <div>
          <div class="conversation-header__title-row">
            <h1>{{ translateUiText(session.title, t) }}</h1>
            <StatusPill :status="session.status" />
          </div>
          <div class="conversation-header__meta">
            <span>{{ translateUiText(session.project, t) }}</span>
            <span><GitBranchIcon :size="13" />{{ session.branch }}</span>
            <span>{{ session.model }}</span>
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
        <XIcon :size="21" />
      </button>
    </header>

    <div class="conversation-body">
      <div class="conversation-summary">
        <span class="conversation-summary__label">{{ t('detail.summary') }}</span>
        <p>{{ translateUiText(session.summary, t) }}</p>
        <div class="conversation-summary__stats">
          <span><Clock3Icon :size="13" /> {{ formatDuration(session.startedAtMs, session.updatedAtMs, t) }}</span>
          <span>{{ t('detail.tokens', { count: n(session.tokenCount, 'integer') }) }}</span>
          <span>{{ t('detail.events', { count: session.events.length }, session.events.length) }}</span>
        </div>
      </div>

      <div class="event-timeline">
        <article v-for="event in session.events" :key="event.id" class="event-item" :class="`event-item--${event.kind}`">
          <div class="event-item__rail">
            <span class="event-item__icon"><component :is="eventIcons[event.kind]" :size="14" /></span>
            <span class="event-item__line" />
          </div>
          <div class="event-item__content">
            <div class="event-item__heading">
              <strong>{{ translateUiText(event.title, t) }}</strong>
              <time :datetime="new Date(event.timestampMs).toISOString()">{{ formatEventTime(event.timestampMs, currentLocale) }}</time>
            </div>
            <p>{{ translateUiText(event.body, t) }}</p>
          </div>
        </article>
      </div>
    </div>

    <footer class="conversation-footer">
      <div class="conversation-footer__state">
        <span class="activity-orb" :class="{ 'activity-orb--paused': session.status !== 'working' }">
          <span />
        </span>
        <div>
          <strong>{{ t(statusMessage.title) }}</strong>
          <span>{{ t(statusMessage.body) }}</span>
        </div>
      </div>
    </footer>
  </section>
</template>
