<script setup lang="ts">
import {
  BotIcon,
  CheckIcon,
  CircleAlertIcon,
  RotateCcwIcon,
  TerminalSquareIcon,
  UserIcon,
} from '@lucide/vue'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ActivityGroup, ActivityItem } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { formatEventTime, formatMilliseconds, translateUiText } from '@/i18n/format'

const { group, index } = defineProps<{
  group: ActivityGroup
  index: number
}>()

const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)

const itemIcons: Record<ActivityItem['kind'], typeof CheckIcon> = {
  user: UserIcon,
  assistant: CheckIcon,
  tool: TerminalSquareIcon,
  system: RotateCcwIcon,
  subagent: BotIcon,
}

const durationMs = computed(() => {
  if (!group.completedAtMs) return undefined
  return Math.max(0, group.completedAtMs - group.startedAtMs)
})

function statusIcon(item: ActivityItem) {
  return item.status === 'error' ? CircleAlertIcon : itemIcons[item.kind]
}
</script>

<template>
  <article class="activity-group">
    <header class="activity-group__header">
      <div>
        <span>{{ t('detail.turn', { count: index + 1 }) }}</span>
        <time :datetime="new Date(group.startedAtMs).toISOString()">
          {{ formatEventTime(group.startedAtMs, currentLocale) }}
        </time>
      </div>
      <span v-if="durationMs" class="activity-group__duration">
        {{ formatMilliseconds(durationMs, currentLocale) }}
      </span>
    </header>

    <div class="activity-group__items">
      <div
        v-for="item in group.items"
        :key="item.id"
        class="activity-item"
        :class="[`activity-item--${item.kind}`, { 'activity-item--error': item.status === 'error' }]"
      >
        <span class="activity-item__icon" aria-hidden="true">
          <component :is="statusIcon(item)" :size="15" />
        </span>
        <div class="activity-item__content">
          <div class="activity-item__heading">
            <strong>{{ translateUiText(item.title, t) }}</strong>
            <span v-if="item.count && item.count > 1">×{{ item.count }}</span>
            <span v-if="item.status" class="activity-item__status" :class="`activity-item__status--${item.status}`">
              {{ t(`detail.activityStatus.${item.status}`) }}
            </span>
          </div>
          <p>{{ translateUiText(item.body, t) }}</p>
        </div>
      </div>
    </div>
  </article>
</template>
