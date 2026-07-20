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
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ProviderMark from '@/components/ProviderMark.vue'
import StatusPill from '@/components/StatusPill.vue'
import type { AgentSession, SessionEvent } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { formatDuration, formatEventTime, translateUiText } from '@/i18n/format'

const { t, n, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)

const { compact = false } = defineProps<{
  session: AgentSession
  compact?: boolean
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
</script>

<template>
  <section
    class="conversation-panel"
    :aria-label="t('detail.aria')"
    :role="compact ? 'dialog' : undefined"
    :aria-modal="compact ? 'true' : undefined"
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
          <strong>{{ t(session.status === 'working' ? 'detail.workingTitle' : 'detail.waitingTitle') }}</strong>
          <span>{{ t(session.status === 'working' ? 'detail.workingBody' : 'detail.waitingBody') }}</span>
        </div>
      </div>
    </footer>
  </section>
</template>
