<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { TokenUsage } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { formatCompactNumber } from '@/i18n/format'

const { usage } = defineProps<{
  usage?: TokenUsage
}>()

const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)

const parts = computed(() => {
  if (!usage) return []
  return [
    ['input', usage.input],
    ['cachedInput', usage.cachedInput],
    ['cacheWriteInput', usage.cacheWriteInput],
    ['output', usage.output],
    ['reasoningOutput', usage.reasoningOutput],
  ].filter((entry): entry is [string, number] => typeof entry[1] === 'number')
})
</script>

<template>
  <div v-if="usage" class="token-breakdown">
    <div class="token-breakdown__total">
      <span>{{ t('detail.tokensTotal') }}</span>
      <strong>{{ formatCompactNumber(usage.total, currentLocale) }}</strong>
    </div>
    <div v-if="parts.length" class="token-breakdown__parts">
      <div v-for="[key, value] in parts" :key="key">
        <span>{{ t(`detail.tokenPart.${key}`) }}</span>
        <strong>{{ formatCompactNumber(value, currentLocale) }}</strong>
      </div>
    </div>
    <p>{{ t(usage.quality === 'detailed' ? 'detail.tokensDetailed' : 'detail.tokensTotalOnly') }}</p>
  </div>
  <div v-else class="token-breakdown token-breakdown--empty">
    <strong>{{ t('detail.unavailable') }}</strong>
    <p>{{ t('detail.tokensUnavailable') }}</p>
  </div>
</template>
