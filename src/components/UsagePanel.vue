<script setup lang="ts">
import { GaugeIcon, TrendingUpIcon } from '@lucide/vue'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ProviderMark from '@/components/ProviderMark.vue'
import type { ProviderDefinition, ProviderQuota } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { formatResetTime, formatUsageCost, translateUiText } from '@/i18n/format'

const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)

const { quotas, providers } = defineProps<{
  quotas: ProviderQuota[]
  providers: ProviderDefinition[]
}>()

function providerName(providerId: ProviderQuota['provider']) {
  return providers.find((provider) => provider.id === providerId)?.name ?? providerId
}

function meterTone(value: number) {
  if (value >= 75) return 'danger'
  if (value >= 60) return 'warning'
  return 'default'
}

function formatPercent(value: number) {
  return new Intl.NumberFormat(currentLocale.value === 'fr' ? 'fr-FR' : 'en-US', {
    style: 'percent',
    maximumFractionDigits: 0,
  }).format(value / 100)
}
</script>

<template>
  <aside class="usage-panel" :aria-label="t('usage.aria')">
    <div class="panel-heading panel-heading--usage">
      <div>
        <p class="panel-heading__eyebrow">{{ t('usage.eyebrow') }}</p>
        <h2>{{ t('usage.title') }}</h2>
      </div>
      <GaugeIcon :size="18" />
    </div>

    <div class="quota-list">
      <article v-for="quota in quotas" :key="quota.provider" class="quota-card">
        <header class="quota-card__header">
          <div class="quota-card__provider">
            <ProviderMark :provider="quota.provider" size="md" />
            <div>
              <strong>{{ providerName(quota.provider) }}</strong>
              <span>{{ quota.plan }}</span>
            </div>
          </div>

          <div v-if="quota.cost" class="quota-card__footer">
            <TrendingUpIcon :size="16" />
            <span>{{ formatUsageCost(quota.cost, currentLocale, t) }}</span>
          </div>
        </header>

        <div class="quota-card__windows">
          <div v-for="window in quota.windows" :key="window.id" class="quota-window">
            <div class="quota-window__labels">
              <span>{{ translateUiText(window.label, t) }}</span>
              <strong>{{ formatPercent(window.usedPercent) }}</strong>
            </div>
            <div
              class="quota-meter"
              :class="`quota-meter--${meterTone(window.usedPercent)}`"
              role="progressbar"
              :aria-label="t('usage.progress', { provider: providerName(quota.provider), label: translateUiText(window.label, t) })"
              aria-valuemin="0"
              aria-valuemax="100"
              :aria-valuenow="window.usedPercent"
            >
              <span :style="{ width: `${window.usedPercent}%` }" />
            </div>
            <span class="quota-window__reset">
              {{ window.resetAtMs
                ? formatResetTime(window.resetAtMs, currentLocale, t)
                : translateUiText(window.resetText ?? { key: 'quota.resetUnknown' }, t) }}
            </span>
          </div>
        </div>

      </article>
    </div>

  </aside>
</template>
