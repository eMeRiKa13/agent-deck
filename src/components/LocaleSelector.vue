<script setup lang="ts">
import { computed } from 'vue'
import { ChevronDownIcon, LanguagesIcon } from '@lucide/vue'
import { useI18n } from 'vue-i18n'
import { setAppLocale, type SupportedLocale } from '@/i18n'

const { locale, t } = useI18n({ useScope: 'global' })

const selectedLocale = computed({
  get: () => locale.value as SupportedLocale,
  set: (value: SupportedLocale) => setAppLocale(value),
})
</script>

<template>
  <label class="locale-selector" :title="t('topbar.language')">
    <LanguagesIcon :size="17" />
    <span>{{ selectedLocale.toUpperCase() }}</span>
    <ChevronDownIcon :size="13" />
    <select v-model="selectedLocale" :aria-label="t('topbar.language')" data-locale-selector>
      <option value="fr">{{ t('language.fr') }}</option>
      <option value="en">{{ t('language.en') }}</option>
    </select>
  </label>
</template>
