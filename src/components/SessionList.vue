<script setup lang="ts">
import { CheckIcon, SearchIcon, SlidersHorizontalIcon } from '@lucide/vue'
import { computed, nextTick, ref, useTemplateRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { onClickOutside, onKeyStroke } from '@vueuse/core'
import ProviderMark from '@/components/ProviderMark.vue'
import StatusPill from '@/components/StatusPill.vue'
import type { AgentSession, SessionStatus } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { formatDuration, formatRelativeTime, translateUiText } from '@/i18n/format'

const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)

defineProps<{
  sessions: AgentSession[]
  selectedId: string
}>()

const search = defineModel<string>('search', { required: true })
const statusFilter = defineModel<SessionStatus[]>('statusFilter', { required: true })
const isFilterOpen = ref(false)
const filterContainer = useTemplateRef<HTMLElement>('filterContainer')
const filterTrigger = useTemplateRef<HTMLButtonElement>('filterTrigger')
const allStatusesOption = useTemplateRef<HTMLButtonElement>('allStatusesOption')
const statusOptions: SessionStatus[] = ['working', 'waiting', 'completed', 'attention']

const filterButtonLabel = computed(() => {
  const count = statusFilter.value.length
  return count === 0
    ? t('sessions.filter')
    : t('sessions.filterActive', { count }, count)
})

async function openFilterMenu() {
  isFilterOpen.value = true
  await nextTick()
  allStatusesOption.value?.focus()
}

function closeFilterMenu(restoreFocus = false) {
  if (!isFilterOpen.value) return
  isFilterOpen.value = false
  if (restoreFocus) nextTick(() => filterTrigger.value?.focus())
}

function toggleFilterMenu() {
  if (isFilterOpen.value) {
    closeFilterMenu()
  } else {
    void openFilterMenu()
  }
}

function toggleStatus(status: SessionStatus) {
  statusFilter.value = statusFilter.value.includes(status)
    ? statusFilter.value.filter((candidate) => candidate !== status)
    : [...statusFilter.value, status]
}

function clearStatusFilter() {
  statusFilter.value = []
}

function handleMenuKeydown(event: KeyboardEvent) {
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return

  const menu = event.currentTarget as HTMLElement
  const options = Array.from(menu.querySelectorAll<HTMLButtonElement>('.status-filter-option'))
  const currentIndex = options.indexOf(document.activeElement as HTMLButtonElement)
  let nextIndex = currentIndex

  if (event.key === 'Home') nextIndex = 0
  if (event.key === 'End') nextIndex = options.length - 1
  if (event.key === 'ArrowDown') nextIndex = currentIndex < options.length - 1 ? currentIndex + 1 : 0
  if (event.key === 'ArrowUp') nextIndex = currentIndex > 0 ? currentIndex - 1 : options.length - 1

  event.preventDefault()
  options[nextIndex]?.focus()
}

onClickOutside(filterContainer, () => closeFilterMenu())
onKeyStroke('Escape', () => closeFilterMenu(true))

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

    <div ref="filterContainer" class="session-search">
      <SearchIcon :size="15" />
      <input v-model="search" type="search" :placeholder="t('sessions.search')" :aria-label="t('sessions.search')" />
      <button
        ref="filterTrigger"
        type="button"
        class="session-filter-trigger"
        :class="{ 'session-filter-trigger--active': statusFilter.length > 0 }"
        :aria-label="filterButtonLabel"
        aria-haspopup="menu"
        :aria-expanded="isFilterOpen"
        aria-controls="session-status-filter"
        data-session-filter
        @click="toggleFilterMenu"
      >
        <SlidersHorizontalIcon :size="15" />
        <span v-if="statusFilter.length > 0" class="session-filter-trigger__count">
          {{ statusFilter.length }}
        </span>
      </button>

      <div
        v-if="isFilterOpen"
        id="session-status-filter"
        class="status-filter-menu"
        role="menu"
        :aria-label="t('sessions.filterStatuses')"
        data-status-filter-menu
        @keydown="handleMenuKeydown"
      >
        <button
          ref="allStatusesOption"
          type="button"
          class="status-filter-option"
          :class="{ 'status-filter-option--selected': statusFilter.length === 0 }"
          role="menuitemcheckbox"
          :aria-checked="statusFilter.length === 0"
          data-status-filter="all"
          @click="clearStatusFilter"
        >
          <span>{{ t('sessions.allStatuses') }}</span>
          <CheckIcon v-if="statusFilter.length === 0" :size="16" aria-hidden="true" />
        </button>
        <button
          v-for="status in statusOptions"
          :key="status"
          type="button"
          class="status-filter-option"
          :class="{ 'status-filter-option--selected': statusFilter.includes(status) }"
          role="menuitemcheckbox"
          :aria-checked="statusFilter.includes(status)"
          :data-status-filter="status"
          @click="toggleStatus(status)"
        >
          <StatusPill :status />
          <CheckIcon v-if="statusFilter.includes(status)" :size="16" aria-hidden="true" />
        </button>
      </div>
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
