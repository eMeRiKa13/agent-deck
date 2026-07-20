import { createI18n } from 'vue-i18n'
import { en, fr } from '@/i18n/messages'

export type SupportedLocale = 'fr' | 'en'

export const LOCALE_STORAGE_KEY = 'agent-deck:locale'
export const supportedLocales: SupportedLocale[] = ['fr', 'en']

export function isSupportedLocale(value: unknown): value is SupportedLocale {
  return value === 'fr' || value === 'en'
}

export function resolveInitialLocale(
  storedLocale = typeof localStorage === 'undefined' ? null : localStorage.getItem(LOCALE_STORAGE_KEY),
  systemLanguage = typeof navigator === 'undefined' ? 'en' : navigator.language,
): SupportedLocale {
  if (isSupportedLocale(storedLocale)) return storedLocale
  return systemLanguage.toLowerCase().startsWith('fr') ? 'fr' : 'en'
}

const initialLocale = resolveInitialLocale()

export const i18n = createI18n({
  legacy: false,
  locale: initialLocale,
  fallbackLocale: 'en',
  messages: { fr, en },
  numberFormats: {
    fr: { integer: { maximumFractionDigits: 0 } },
    en: { integer: { maximumFractionDigits: 0 } },
  },
})

export function setAppLocale(locale: SupportedLocale, persist = true) {
  i18n.global.locale.value = locale
  if (typeof document !== 'undefined') document.documentElement.lang = locale
  if (persist && typeof localStorage !== 'undefined') localStorage.setItem(LOCALE_STORAGE_KEY, locale)
}

setAppLocale(initialLocale, false)
