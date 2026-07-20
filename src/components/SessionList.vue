<script setup lang="ts">
import { SearchIcon, SlidersHorizontalIcon } from '@lucide/vue'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ProviderMark from '@/components/ProviderMark.vue'
import StatusPill from '@/components/StatusPill.vue'
import type { AgentSession } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { formatDuration, formatRelativeTime, translateUiText } from '@/i18n/format'

const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)

defineProps<{
  sessions: AgentSession[]
  selectedId: string
}>()

const search = defineModel<string>('search', { required: true })

const emit = defineEmits<{
  select: [id: string]
}>()
</script>

<template>
  <section class="session-column" :aria-label="t('sessions.aria')">
    <div class="panel-heading">
      <div>
        <h2>{{ t('sessions.recent') }}</h2>
      </div>
      <span class="panel-heading__count">{{ sessions.length }}</span>
    </div>

    <div class="session-search">
      <SearchIcon :size="15" />
      <input v-model="search" type="search" :placeholder="t('sessions.search')" :aria-label="t('sessions.search')" />
      <button type="button" :aria-label="t('sessions.filtersUnavailable')" data-session-filter disabled>
        <SlidersHorizontalIcon :size="15" />
      </button>
    </div>

    <div class="session-list">
      <button
        v-for="session in sessions"
        :key="session.id"
        type="button"
        class="session-row"
        :class="{ 'session-row--selected': selectedId === session.id }"
        data-session-row
        @click="emit('select', session.id)"
      >
        <ProviderMark :provider="session.provider" size="md" />
        <span class="session-row__body">
          <span class="session-row__topline">
            <strong>{{ translateUiText(session.title, t) }}</strong>
            <span>{{ formatRelativeTime(session.updatedAtMs, currentLocale, t) }}</span>
          </span>
          <span class="session-row__summary">{{ translateUiText(session.summary, t) }}</span>
          <span class="session-row__meta">
            <StatusPill :status="session.status" />
            <span>{{ translateUiText(session.project, t) }}</span>
            <span>{{ formatDuration(session.startedAtMs, session.updatedAtMs, t) }}</span>
          </span>
        </span>
      </button>

      <div v-if="sessions.length === 0" class="session-list__empty">
        {{ t('sessions.empty') }}
      </div>
    </div>
  </section>
</template>
