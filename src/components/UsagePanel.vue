<script setup lang="ts">
import { GaugeIcon, TrendingUpIcon } from '@lucide/vue'
import ProviderMark from '@/components/ProviderMark.vue'
import type { ProviderDefinition, ProviderQuota } from '@/domain/dashboard'

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
</script>

<template>
  <aside class="usage-panel" aria-label="Quotas d’utilisation">
    <div class="panel-heading panel-heading--usage">
      <div>
        <p class="panel-heading__eyebrow">Abonnements</p>
        <h2>Utilisation</h2>
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

          <div v-if="quota.costLabel" class="quota-card__footer">
            <TrendingUpIcon :size="16" />
            <span>{{ quota.costLabel }}</span>
          </div>
        </header>

        <div class="quota-card__windows">
          <div v-for="window in quota.windows" :key="window.id" class="quota-window">
            <div class="quota-window__labels">
              <span>{{ window.label }}</span>
              <strong>{{ window.usedPercent }} %</strong>
            </div>
            <div
              class="quota-meter"
              :class="`quota-meter--${meterTone(window.usedPercent)}`"
              role="progressbar"
              :aria-label="`${providerName(quota.provider)} — ${window.label}`"
              aria-valuemin="0"
              aria-valuemax="100"
              :aria-valuenow="window.usedPercent"
            >
              <span :style="{ width: `${window.usedPercent}%` }" />
            </div>
            <span class="quota-window__reset">{{ window.resetLabel }}</span>
          </div>
        </div>

      </article>
    </div>

  </aside>
</template>
