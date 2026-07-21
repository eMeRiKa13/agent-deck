<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { CircleAlertIcon, MoonIcon, RefreshCwIcon, SunIcon, WifiIcon } from '@lucide/vue'
import { onKeyStroke, useColorMode, useMediaQuery } from '@vueuse/core'
import AboutPanel from '@/components/AboutPanel.vue'
import AppSidebar from '@/components/AppSidebar.vue'
import LocaleSelector from '@/components/LocaleSelector.vue'
import SessionDetail from '@/components/SessionDetail.vue'
import SessionList from '@/components/SessionList.vue'
import UsagePanel from '@/components/UsagePanel.vue'
import { useDashboard } from '@/composables/useDashboard'
import type { SupportedLocale } from '@/i18n'
import { formatRelativeTime } from '@/i18n/format'

const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)

const mode = useColorMode()
const {
  snapshot,
  sessions,
  selectedSession,
  selectedSessionId,
  activeProvider,
  activeCount,
  attentionCount,
  search,
  refreshing,
  pollingIntervalSeconds,
  error,
  refresh,
  selectProvider,
  selectSession,
} = useDashboard()

const isDark = computed(() => mode.value === 'dark')
const isCompactDetail = useMediaQuery('(max-width: 1799px)')
const isSessionDetailOpen = ref(false)
const isAboutOpen = ref(false)
const isSessionDetailModal = computed(() => isCompactDetail.value && isSessionDetailOpen.value)

watch(isCompactDetail, () => {
  isSessionDetailOpen.value = false
})

watch(selectedSession, (session) => {
  if (!session) isSessionDetailOpen.value = false
}, { flush: 'sync' })

onKeyStroke('Escape', () => {
  if (isAboutOpen.value) {
    isAboutOpen.value = false
    return
  }
  isSessionDetailOpen.value = false
})

function toggleTheme() {
  mode.value = isDark.value ? 'light' : 'dark'
}

function handleSelectSession(id: string) {
  selectSession(id)
  if (isCompactDetail.value) isSessionDetailOpen.value = true
}

function openAbout() {
  isSessionDetailOpen.value = false
  isAboutOpen.value = true
}
</script>

<template>
  <div v-if="snapshot" class="dashboard-shell" :inert="isAboutOpen">
    <AppSidebar
      :providers="snapshot.providers"
      :active-provider="activeProvider"
      :active-count="activeCount"
      :inert="isSessionDetailModal"
      @select-provider="selectProvider"
      @open-about="openAbout"
    />

    <main class="dashboard-main">
      <header data-tauri-drag-region class="dashboard-topbar" :inert="isSessionDetailModal">
        <div class="dashboard-topbar__title">
          <div>
            <h1>{{ t('topbar.title') }}</h1>
            <span>
              {{ t('topbar.activeAgents', { count: activeCount }, activeCount) }} ·
              {{ t('topbar.requiredActions', { count: attentionCount }, attentionCount) }}
            </span>
          </div>
          <span class="live-badge"><WifiIcon :size="12" /> {{ t('topbar.autoRefresh', { seconds: pollingIntervalSeconds }) }}</span>
        </div>
        <div class="dashboard-topbar__actions">
          <span class="last-refresh">{{ t('topbar.updated', { time: formatRelativeTime(snapshot.refreshedAtMs, currentLocale, t) }) }}</span>
          <button type="button" :class="{ 'is-spinning': refreshing }" :aria-label="t('topbar.refresh')" @click="refresh">
            <RefreshCwIcon :size="17" />
          </button>
          <button type="button" :aria-label="t('topbar.theme')" @click="toggleTheme">
            <SunIcon v-if="isDark" :size="17" />
            <MoonIcon v-else :size="17" />
          </button>
          <LocaleSelector />
        </div>
      </header>

      <div v-if="error" class="refresh-error" role="status" aria-live="polite">
        <CircleAlertIcon :size="17" aria-hidden="true" />
        <span>{{ t('app.refreshError', { error }) }}</span>
      </div>

      <div class="dashboard-workspace">
        <SessionList
          v-model:search="search"
          :sessions
          :selected-id="selectedSessionId"
          :inert="isSessionDetailModal"
          @select="handleSelectSession"
        />
        <SessionDetail
          v-if="selectedSession && !isCompactDetail"
          :session="selectedSession"
        />
        <UsagePanel
          :quotas="snapshot.quotas"
          :providers="snapshot.providers"
          :inert="isSessionDetailModal"
        />
      </div>
    </main>
  </div>

  <div v-else-if="error" class="dashboard-loading">
    {{ t('app.loadError', { error }) }}
  </div>

  <div v-else class="dashboard-loading">
    <span />
    {{ t('app.loading') }}
  </div>

  <Teleport to="body">
    <div
      v-if="selectedSession && isCompactDetail && isSessionDetailOpen"
      class="conversation-overlay"
    >
      <button
        type="button"
        class="conversation-backdrop"
        :aria-label="t('detail.close')"
        @click="isSessionDetailOpen = false"
      />
      <SessionDetail
        :session="selectedSession"
        compact
        open
        class="conversation-panel--open"
        @close="isSessionDetailOpen = false"
      />
    </div>
  </Teleport>

  <AboutPanel :open="isAboutOpen" @close="isAboutOpen = false" />
</template>
