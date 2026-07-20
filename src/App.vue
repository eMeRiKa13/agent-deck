<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { MoonIcon, RefreshCwIcon, SunIcon, WifiIcon } from '@lucide/vue'
import { onKeyStroke, useColorMode, useMediaQuery } from '@vueuse/core'
import AppSidebar from '@/components/AppSidebar.vue'
import SessionDetail from '@/components/SessionDetail.vue'
import SessionList from '@/components/SessionList.vue'
import UsagePanel from '@/components/UsagePanel.vue'
import { useDashboard } from '@/composables/useDashboard'

const mode = useColorMode({ emitAuto: true })
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
const isSessionDetailVisible = computed(() => !isCompactDetail.value || isSessionDetailOpen.value)

watch(isCompactDetail, () => {
  isSessionDetailOpen.value = false
})

onKeyStroke('Escape', () => {
  isSessionDetailOpen.value = false
})

function toggleTheme() {
  mode.value = isDark.value ? 'light' : 'dark'
}

function handleSelectSession(id: string) {
  selectSession(id)
  if (isCompactDetail.value) isSessionDetailOpen.value = true
}
</script>

<template>
  <div v-if="snapshot" class="dashboard-shell">
    <AppSidebar
      :providers="snapshot.providers"
      :active-provider="activeProvider"
      :active-count="activeCount"
      @select-provider="selectProvider"
    />

    <main class="dashboard-main">
      <header data-tauri-drag-region class="dashboard-topbar">
        <div class="dashboard-topbar__title">
          <div>
            <h1>Centre de contrôle</h1>
            <span>{{ activeCount }} agents actifs · {{ attentionCount }} actions requises</span>
          </div>
          <span class="live-badge"><WifiIcon :size="12" /> Actualisation auto · {{ pollingIntervalSeconds }} s</span>
        </div>
        <div class="dashboard-topbar__actions">
          <span class="last-refresh">Mis à jour {{ snapshot.refreshedAt }}</span>
          <button type="button" :class="{ 'is-spinning': refreshing }" aria-label="Rafraîchir" @click="refresh">
            <RefreshCwIcon :size="17" />
          </button>
          <button type="button" aria-label="Changer de thème" @click="toggleTheme">
            <SunIcon v-if="isDark" :size="17" />
            <MoonIcon v-else :size="17" />
          </button>
        </div>
      </header>

      <div class="dashboard-workspace">
        <SessionList
          v-model:search="search"
          :sessions
          :selected-id="selectedSessionId"
          @select="handleSelectSession"
        />
        <button
          v-if="isCompactDetail && isSessionDetailOpen"
          type="button"
          class="conversation-backdrop"
          aria-label="Fermer le détail de la session"
          @click="isSessionDetailOpen = false"
        />
        <SessionDetail
          v-if="selectedSession"
          :session="selectedSession"
          :compact="isCompactDetail"
          :class="{ 'conversation-panel--open': isSessionDetailOpen }"
          :inert="!isSessionDetailVisible"
          @close="isSessionDetailOpen = false"
        />
        <UsagePanel :quotas="snapshot.quotas" :providers="snapshot.providers" />
      </div>
    </main>
  </div>

  <div v-else-if="error" class="dashboard-loading">
    Impossible de lire les données locales : {{ error }}
  </div>

  <div v-else class="dashboard-loading">
    <span />
    Chargement des agents…
  </div>
</template>
