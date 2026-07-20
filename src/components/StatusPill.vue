<script setup lang="ts">
import type { SessionStatus } from '@/domain/dashboard'

const { status, compact = false } = defineProps<{
  status: SessionStatus
  compact?: boolean
}>()

const labels: Record<SessionStatus, string> = {
  working: 'En cours',
  waiting: 'En attente',
  completed: 'Terminé',
  attention: 'À valider',
}
</script>

<template>
  <span class="status-pill" :class="[`status-pill--${status}`, { 'status-pill--compact': compact }]">
    <span class="status-pill__dot" />
    <span v-if="!compact">{{ labels[status] }}</span>
  </span>
</template>
