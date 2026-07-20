<script setup lang="ts">
import { SearchIcon, SlidersHorizontalIcon } from '@lucide/vue'
import ProviderMark from '@/components/ProviderMark.vue'
import StatusPill from '@/components/StatusPill.vue'
import type { AgentSession } from '@/domain/dashboard'

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
  <section class="session-column" aria-label="Sessions">
    <div class="panel-heading">
      <div>
        <h2>Sessions récentes</h2>
      </div>
      <span class="panel-heading__count">{{ sessions.length }}</span>
    </div>

    <div class="session-search">
      <SearchIcon :size="15" />
      <input v-model="search" type="search" placeholder="Rechercher une session" aria-label="Rechercher une session" />
      <button type="button" aria-label="Filtres indisponibles" data-session-filter disabled>
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
            <strong>{{ session.title }}</strong>
            <span>{{ session.updatedAt }}</span>
          </span>
          <span class="session-row__summary">{{ session.summary }}</span>
          <span class="session-row__meta">
            <StatusPill :status="session.status" />
            <span>{{ session.project }}</span>
            <span>{{ session.elapsed }}</span>
          </span>
        </span>
      </button>

      <div v-if="sessions.length === 0" class="session-list__empty">
        Aucune session ne correspond à cette recherche.
      </div>
    </div>
  </section>
</template>
