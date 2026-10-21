<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { QuotaWindow } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { formatResetCountdown, formatResetTime, localeTag, translateUiText } from '@/i18n/format'

const props = defineProps<{ window: QuotaWindow; provider: string; nowMs: number; absolute?: boolean }>()
const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)
const percent = computed(() => new Intl.NumberFormat(localeTag(currentLocale.value), {
  style: 'percent', maximumFractionDigits: 0,
}).format(props.window.usedPercent / 100))
const tone = computed(() => props.window.usedPercent >= 75 ? 'danger' : props.window.usedPercent >= 60 ? 'warning' : 'default')
const label = computed(() => props.window.id === 'cursor-auto' ? t('quota.cursorModels')
  : props.window.id === 'cursor-api' ? t('quota.otherModels') : translateUiText(props.window.label, t))
</script>

<template>
  <div class="quota-window">
    <div class="quota-window__labels">
      <span>{{ label }}</span>
      <strong>{{ percent }}<small>{{ t('usage.used') }}</small></strong>
    </div>
    <div class="quota-meter" :class="`quota-meter--${tone}`" role="progressbar"
      :aria-label="t('usage.progress', { provider: props.provider, label })"
      aria-valuemin="0" aria-valuemax="100" :aria-valuenow="props.window.usedPercent">
      <span :style="{ width: `${props.window.usedPercent}%` }" />
    </div>
    <span class="quota-window__reset" :title="props.window.resetAtMs
      ? formatResetTime(props.window.resetAtMs, currentLocale, t) : undefined">
      {{ props.window.resetAtMs ? (props.absolute ? formatResetTime(props.window.resetAtMs, currentLocale, t)
        : formatResetCountdown(props.window.resetAtMs, props.nowMs, t))
        : translateUiText(props.window.resetText ?? { key: 'quota.resetUnknown' }, t) }}
    </span>
  </div>
</template>
