<script setup lang="ts">
import type { SessionStatus } from '@/domain/dashboard'
import { useI18n } from 'vue-i18n'

const { t } = useI18n({ useScope: 'global' })

const { status, compact = false } = defineProps<{
  status: SessionStatus
  compact?: boolean
}>()

</script>

<template>
  <span class="status-pill" :class="[`status-pill--${status}`, { 'status-pill--compact': compact }]">
    <span class="status-pill__dot" />
    <span v-if="!compact">{{ t(`status.${status}`) }}</span>
  </span>
</template>
