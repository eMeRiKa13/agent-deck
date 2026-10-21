<script setup lang="ts">
import { ChevronDownIcon, GaugeIcon, GiftIcon, InfoIcon, TrendingUpIcon } from '@lucide/vue'
import { computed } from 'vue'
import { useNow } from '@vueuse/core'
import { useI18n } from 'vue-i18n'
import ProviderMark from '@/components/ProviderMark.vue'
import QuotaWindowMeter from '@/components/QuotaWindowMeter.vue'
import type { ProviderDefinition, ProviderQuota } from '@/domain/dashboard'
import type { SupportedLocale } from '@/i18n'
import { formatOfferExpiry, formatOfferExpiryTime, formatResetTime, formatUsageCost, translateUiText } from '@/i18n/format'

const props = defineProps<{ quotas: ProviderQuota[]; providers: ProviderDefinition[] }>()
const { t, locale } = useI18n({ useScope: 'global' })
const currentLocale = computed(() => locale.value as SupportedLocale)
const now = useNow({ interval: 30_000 })

// Classify stable provider window IDs rather than localized labels or percentages.
function primaryWindows(quota: ProviderQuota) {
  if (quota.provider === 'claude') {
    const regular = quota.windows.filter((window) => ['claude-session', 'claude-week'].includes(window.id))
    return regular.length ? regular : quota.windows
  }
  if (quota.provider === 'cursor') {
    const breakdown = quota.windows.filter((window) => ['cursor-auto', 'cursor-api'].includes(window.id))
    if (breakdown.length > 0) return breakdown
  }
  return quota.windows
}

const cards = computed(() => props.quotas.map((quota) => {
  const primary = primaryWindows(quota)
  const secondary = quota.windows.filter((window) => !primary.includes(window))
  const offers = (quota.resetOffers ?? []).filter((offer) => offer.count > 0
    && (!offer.expiresAtMs || offer.expiresAtMs > now.value.getTime()))
  const expiries = offers.flatMap((offer) => offer.expiresAtMs ? [offer.expiresAtMs] : [])
  return {
    quota, primary, secondary, offers,
    name: props.providers.find((provider) => provider.id === quota.provider)?.name ?? quota.provider,
    offerCount: offers.reduce((count, offer) => count + offer.count, 0),
    nextExpiry: expiries.length ? Math.min(...expiries) : undefined,
  }
}))
</script>

<template>
  <aside class="usage-panel" :aria-label="t('usage.aria')">
    <div class="panel-heading panel-heading--usage">
      <h2>{{ t('usage.title') }}</h2>
      <div class="usage-panel__unit"><span>{{ t('usage.percentUsed') }}</span><GaugeIcon :size="18" /></div>
    </div>
    <div class="quota-list">
      <article v-for="card in cards" :key="card.quota.provider" class="quota-card" :data-provider="card.quota.provider">
        <header class="quota-card__header">
          <div class="quota-card__provider">
            <ProviderMark :provider="card.quota.provider" size="md" />
            <div><strong>{{ card.name }}</strong><span>{{ card.quota.plan }}</span></div>
          </div>
          <span v-if="card.quota.provider === 'codex'" class="quota-card__context" :title="t('usage.codexScope')">
            <InfoIcon :size="14" aria-hidden="true" />{{ t('usage.shared') }}
          </span>
        </header>
        <div v-if="card.primary.length" class="quota-card__windows" :class="{ 'quota-card__windows--single': card.primary.length === 1 }">
          <QuotaWindowMeter v-for="window in card.primary" :key="window.id" :window="window" :provider="card.name" :now-ms="now.getTime()" />
        </div>
        <div v-if="card.offerCount" class="quota-reset-offer" :class="{ 'quota-reset-offer--soon': card.nextExpiry && card.nextExpiry - now.getTime() < 86_400_000 }">
          <GiftIcon :size="15" aria-hidden="true" />
          <span>{{ t('usage.resetOffers', { count: card.offerCount }, card.offerCount) }}<small v-if="card.nextExpiry"
            :title="formatOfferExpiryTime(card.nextExpiry, currentLocale, t)">
            {{ card.offerCount > 1 ? t('usage.nextExpiry') : '' }}{{ formatOfferExpiry(card.nextExpiry, now.getTime(), currentLocale, t) }}
          </small></span>
        </div>
        <details v-if="card.primary.some((window) => window.resetAtMs) || card.secondary.length || card.quota.cost || card.offerCount || card.quota.provider === 'codex'" class="quota-details">
          <summary>{{ card.secondary.length && card.quota.provider === 'claude' ? t('usage.modelLimits') : t('usage.details') }}<ChevronDownIcon :size="15" aria-hidden="true" /></summary>
          <div class="quota-details__body">
            <p v-if="card.quota.provider === 'codex'" class="quota-details__scope">{{ t('usage.codexScope') }}</p>
            <dl class="quota-details__dates">
              <div v-for="window in card.primary.filter((window) => window.resetAtMs)" :key="window.id">
                <dt>{{ window.id === 'cursor-auto' ? t('quota.cursorModels') : window.id === 'cursor-api' ? t('quota.otherModels') : translateUiText(window.label, t) }}</dt>
                <dd>{{ formatResetTime(window.resetAtMs!, currentLocale, t) }}</dd>
              </div>
            </dl>
            <div v-if="card.secondary.length" class="quota-card__windows" :class="{ 'quota-card__windows--single': card.secondary.length === 1 }">
              <QuotaWindowMeter v-for="window in card.secondary" :key="window.id" :window="window" :provider="card.name" :now-ms="now.getTime()" absolute />
            </div>
            <div v-if="card.quota.cost" class="quota-details__cost"><TrendingUpIcon :size="15" aria-hidden="true" />{{ formatUsageCost(card.quota.cost, currentLocale, t) }}</div>
            <ul v-if="card.offers.length" class="quota-details__offers">
              <li v-for="(offer, index) in card.offers" :key="index">{{ t('usage.resetOffers', { count: offer.count }, offer.count) }} · {{ offer.expiresAtMs ? formatOfferExpiryTime(offer.expiresAtMs, currentLocale, t) : t('usage.expiryUnknown') }}</li>
            </ul>
          </div>
        </details>
      </article>
    </div>
  </aside>
</template>
