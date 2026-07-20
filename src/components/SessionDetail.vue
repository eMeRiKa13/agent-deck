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
import ProviderMark from '@/components/ProviderMark.vue'
import StatusPill from '@/components/StatusPill.vue'
import type { AgentSession, SessionEvent } from '@/domain/dashboard'

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
    aria-label="Conversation sélectionnée"
    :role="compact ? 'dialog' : undefined"
    :aria-modal="compact ? 'true' : undefined"
  >
    <header class="conversation-header">
      <div class="conversation-header__identity">
        <ProviderMark :provider="session.provider" size="lg" />
        <div>
          <div class="conversation-header__title-row">
            <h1>{{ session.title }}</h1>
            <StatusPill :status="session.status" />
          </div>
          <div class="conversation-header__meta">
            <span>{{ session.project }}</span>
            <span><GitBranchIcon :size="13" />{{ session.branch }}</span>
            <span>{{ session.model }}</span>
          </div>
        </div>
      </div>
      <button
        v-if="compact"
        type="button"
        class="conversation-panel__close"
        aria-label="Fermer le détail"
        @click="emit('close')"
      >
        <XIcon :size="21" />
      </button>
    </header>

    <div class="conversation-body">
      <div class="conversation-summary">
        <span class="conversation-summary__label">Résumé</span>
        <p>{{ session.summary }}</p>
        <div class="conversation-summary__stats">
          <span><Clock3Icon :size="13" /> {{ session.elapsed }}</span>
          <span>{{ session.tokenCount.toLocaleString('fr-FR') }} tokens</span>
          <span>{{ session.events.length }} événements</span>
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
              <strong>{{ event.title }}</strong>
              <time>{{ event.time }}</time>
            </div>
            <p>{{ event.body }}</p>
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
          <strong>{{ session.status === 'working' ? 'Agent au travail' : 'Session en attente' }}</strong>
          <span>{{ session.status === 'working' ? 'Les événements apparaissent ici en direct' : 'Une action de votre part peut être nécessaire' }}</span>
        </div>
      </div>
    </footer>
  </section>
</template>
