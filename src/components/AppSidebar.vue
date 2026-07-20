<script setup lang="ts">
import {
  ActivityIcon,
  BarChart3Icon,
  LayoutDashboardIcon,
} from '@lucide/vue'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import agentDeckMark from '@/assets/branding/agent-deck-mark.png'
import ProviderMark from '@/components/ProviderMark.vue'
import type { ProviderDefinition, ProviderId } from '@/domain/dashboard'
import { translateUiText } from '@/i18n/format'

defineProps<{
  providers: ProviderDefinition[]
  activeProvider: ProviderId | 'all'
  activeCount: number
}>()

const emit = defineEmits<{
  selectProvider: [provider: ProviderId | 'all']
}>()

const { t } = useI18n({ useScope: 'global' })

const navigation = computed(() => [
  { id: 'overview', label: t('navigation.overview'), icon: LayoutDashboardIcon, active: true },
  { id: 'activity', label: t('navigation.activity'), icon: ActivityIcon, active: false },
  { id: 'usage', label: t('navigation.usage'), icon: BarChart3Icon, active: false },
])

const providerStack: ProviderId[] = ['codex', 'claude', 'cursor']
</script>

<template>
  <aside class="app-sidebar">
    <div data-tauri-drag-region class="app-sidebar__drag" />

    <div class="app-sidebar__brand">
      <span class="app-sidebar__brand-mark" aria-hidden="true">
        <img :src="agentDeckMark" alt="" draggable="false" />
      </span>
      <span>{{ t('app.name') }}</span>
    </div>

    <nav class="app-sidebar__nav" :aria-label="t('navigation.aria')">
      <button
        v-for="item in navigation"
        :key="item.id"
        type="button"
        class="sidebar-nav-item"
        :class="{ 'sidebar-nav-item--active': item.active }"
        :disabled="!item.active"
        :aria-current="item.active ? 'page' : undefined"
        :data-nav-item="item.id"
        :title="item.active ? undefined : t('navigation.unavailable', { label: item.label })"
      >
        <component :is="item.icon" :size="17" />
        <span>{{ item.label }}</span>
        <span v-if="item.active && activeCount" class="sidebar-nav-item__count">{{ activeCount }}</span>
      </button>
    </nav>

    <div class="app-sidebar__section">
      <div class="app-sidebar__section-label">{{ t('navigation.providers') }}</div>
      <button
        type="button"
        class="provider-nav-item"
        :class="{ 'provider-nav-item--active': activeProvider === 'all' }"
        @click="emit('selectProvider', 'all')"
      >
        <span class="provider-stack" aria-hidden="true">
          <ProviderMark
            v-for="provider in providerStack"
            :key="provider"
            :provider
            size="xs"
            class="provider-stack__logo"
          />
        </span>
        <span>{{ t('navigation.allAgents') }}</span>
      </button>

      <button
        v-for="provider in providers"
        :key="provider.id"
        type="button"
        class="provider-nav-item"
        :class="{ 'provider-nav-item--active': activeProvider === provider.id }"
        :title="provider.error ? translateUiText(provider.error, t) : t('navigation.connected', { provider: provider.name })"
        @click="emit('selectProvider', provider.id)"
      >
        <ProviderMark :provider="provider.id" size="sm" />
        <span>{{ provider.name }}</span>
        <span
          class="provider-nav-item__status"
          :class="{
            'provider-nav-item__status--online': provider.connected && !provider.error,
            'provider-nav-item__status--warning': provider.connected && provider.error,
          }"
        />
      </button>
    </div>

    <div class="app-sidebar__footer">
      <div class="local-indicator">
        <span class="local-indicator__dot" />
        {{ t('navigation.localOnly') }}
      </div>
    </div>
  </aside>
</template>
