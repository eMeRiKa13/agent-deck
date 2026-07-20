<script setup lang="ts">
import {
  ActivityIcon,
  BarChart3Icon,
  LayoutDashboardIcon,
} from '@lucide/vue'
import agentDeckMark from '@/assets/branding/agent-deck-mark.png'
import ProviderMark from '@/components/ProviderMark.vue'
import type { ProviderDefinition, ProviderId } from '@/domain/dashboard'

defineProps<{
  providers: ProviderDefinition[]
  activeProvider: ProviderId | 'all'
  activeCount: number
}>()

const emit = defineEmits<{
  selectProvider: [provider: ProviderId | 'all']
}>()

const navigation = [
  { label: 'Vue d’ensemble', icon: LayoutDashboardIcon, active: true },
  { label: 'Activité', icon: ActivityIcon, active: false },
  { label: 'Utilisation', icon: BarChart3Icon, active: false },
]

const providerStack: ProviderId[] = ['codex', 'claude', 'cursor']
</script>

<template>
  <aside class="app-sidebar">
    <div data-tauri-drag-region class="app-sidebar__drag" />

    <div class="app-sidebar__brand">
      <span class="app-sidebar__brand-mark" aria-hidden="true">
        <img :src="agentDeckMark" alt="" draggable="false" />
      </span>
      <span>Agent Deck</span>
    </div>

    <nav class="app-sidebar__nav" aria-label="Navigation principale">
      <button
        v-for="item in navigation"
        :key="item.label"
        type="button"
        class="sidebar-nav-item"
        :class="{ 'sidebar-nav-item--active': item.active }"
        :disabled="!item.active"
        :aria-current="item.active ? 'page' : undefined"
        :data-nav-item="item.label"
        :title="item.active ? undefined : `${item.label} indisponible`"
      >
        <component :is="item.icon" :size="17" />
        <span>{{ item.label }}</span>
        <span v-if="item.active && activeCount" class="sidebar-nav-item__count">{{ activeCount }}</span>
      </button>
    </nav>

    <div class="app-sidebar__section">
      <div class="app-sidebar__section-label">Fournisseurs</div>
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
        <span>Tous les agents</span>
      </button>

      <button
        v-for="provider in providers"
        :key="provider.id"
        type="button"
        class="provider-nav-item"
        :class="{ 'provider-nav-item--active': activeProvider === provider.id }"
        :title="provider.error ?? `${provider.name} connecté`"
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
        Données locales uniquement
      </div>
    </div>
  </aside>
</template>
