import type { ComposerTranslation } from 'vue-i18n'
import type { SupportedLocale } from '@/i18n'
import type { UiText, UsageCost } from '@/domain/dashboard'

type FormatTranslation = (key: string, params?: Record<string, string | number>) => string

/** Map an application locale to the regional tag used by Intl formatters. */
export function localeTag(locale: SupportedLocale) {
  return locale === 'fr' ? 'fr-FR' : 'en-US'
}

/** Resolve raw or keyed backend text through the active translation catalog. */
export function translateUiText(value: UiText, t: ComposerTranslation): string {
  return typeof value === 'string' ? value : t(value.key, value.params ?? {})
}

/** Format session freshness using stable, localized dashboard thresholds. */
export function formatRelativeTime(timestampMs: number, _locale: SupportedLocale, t: ComposerTranslation): string {
  const elapsed = Math.max(0, Math.floor((Date.now() - timestampMs) / 1_000))
  if (elapsed <= 44) return t('time.now')
  if (elapsed <= 89) return t('time.minuteAgo')
  if (elapsed <= 3_599) return t('time.minutesAgo', { count: Math.floor(elapsed / 60) })
  if (elapsed <= 86_399) return t('time.hoursAgo', { count: Math.floor(elapsed / 3_600) })
  return t('time.daysAgo', { count: Math.floor(elapsed / 86_400) })
}

/** Format an elapsed session duration without exposing raw timestamps. */
export function formatDuration(startedAtMs: number, updatedAtMs: number, t: ComposerTranslation): string {
  const elapsed = Math.max(0, Math.floor((updatedAtMs - startedAtMs) / 1_000))
  if (elapsed <= 59) return t('time.lessThanMinute')
  if (elapsed <= 3_599) return t('time.minutes', { count: Math.floor(elapsed / 60) })
  if (elapsed <= 86_399) {
    return t('time.hoursMinutes', {
      hours: Math.floor(elapsed / 3_600),
      minutes: String(Math.floor((elapsed % 3_600) / 60)).padStart(2, '0'),
    })
  }
  return t('time.days', { count: Math.floor(elapsed / 86_400) })
}

/** Format a timeline event using the user's locale and a compact clock. */
export function formatEventTime(timestampMs: number, locale: SupportedLocale): string {
  return new Intl.DateTimeFormat(localeTag(locale), { hour: '2-digit', minute: '2-digit' }).format(timestampMs)
}

/** Format large local counters without suggesting more precision than the source provides. */
export function formatCompactNumber(value: number, locale: SupportedLocale): string {
  return new Intl.NumberFormat(localeTag(locale), {
    notation: value >= 10_000 ? 'compact' : 'standard',
    maximumFractionDigits: value >= 10_000 ? 1 : 0,
  }).format(value)
}

/** Format provider-reported millisecond durations as a compact dashboard value. */
export function formatMilliseconds(value: number, locale: SupportedLocale): string {
  if (value < 1_000) return `${Math.round(value)} ms`
  const seconds = value / 1_000
  if (seconds < 60) {
    return `${new Intl.NumberFormat(localeTag(locale), { maximumFractionDigits: seconds < 10 ? 1 : 0 }).format(seconds)} s`
  }
  const minutes = Math.floor(seconds / 60)
  const remainingSeconds = Math.round(seconds % 60)
  return `${minutes} min ${String(remainingSeconds).padStart(2, '0')} s`
}

/** Format a quota reset deadline with its localized prefix. */
export function formatResetTime(timestampMs: number, locale: SupportedLocale, t: ComposerTranslation): string {
  const date = new Intl.DateTimeFormat(localeTag(locale), {
    weekday: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(timestampMs)
  return t('time.resetAt', { date })
}

/** Format a live countdown without claiming a reset already happened. */
export function formatResetCountdown(timestampMs: number, nowMs: number, t: FormatTranslation): string {
  if (timestampMs <= nowMs) return t('time.resetPending')
  const minutes = Math.floor((timestampMs - nowMs) / 60_000)
  const duration = minutes < 1 ? t('time.lessThanMinute')
    : minutes < 60 ? t('time.minutes', { count: minutes })
      : minutes < 1_440 ? t('time.hoursMinutes', {
        hours: Math.floor(minutes / 60), minutes: String(minutes % 60).padStart(2, '0'),
      }) : t('time.daysHours', { days: Math.floor(minutes / 1_440), hours: Math.floor((minutes % 1_440) / 60) })
  return t('time.resetIn', { duration })
}

/** Use local calendar days for offer expirations, including across daylight-saving changes. */
export function formatOfferExpiry(timestampMs: number, nowMs: number, locale: SupportedLocale, t: FormatTranslation): string {
  const expiry = new Date(timestampMs)
  const today = new Date(nowMs)
  const tomorrow = new Date(nowMs)
  tomorrow.setDate(tomorrow.getDate() + 1)
  const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear()
    && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()
  if (sameDay(expiry, today)) return t('usage.expiresToday')
  if (sameDay(expiry, tomorrow)) return t('usage.expiresTomorrow')
  return t('usage.expiresOn', { date: new Intl.DateTimeFormat(localeTag(locale), { day: 'numeric', month: 'short' }).format(expiry) })
}

export function formatOfferExpiryTime(timestampMs: number, locale: SupportedLocale, t: FormatTranslation): string {
  return t('usage.expiresAt', { date: new Intl.DateTimeFormat(localeTag(locale), {
    day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit',
  }).format(timestampMs) })
}

/** Render every supported usage-cost variant into localized display text. */
export function formatUsageCost(cost: UsageCost, locale: SupportedLocale, t: ComposerTranslation): string {
  const number = (value: number, maximumFractionDigits = 2) =>
    new Intl.NumberFormat(localeTag(locale), { maximumFractionDigits }).format(value)

  switch (cost.kind) {
    case 'credits':
      return t('cost.credits', { value: number(cost.value) })
    case 'extraUsage':
      return t('cost.extraUsage', { value: number(cost.value), currency: cost.currency })
    case 'includedOnDemand':
      return t('cost.includedOnDemand', {
        included: number(cost.included),
        onDemand: number(cost.onDemand),
      })
    case 'localMeasurement':
      return t('cost.localMeasurement', {
        time: formatRelativeTime(cost.measuredAtMs, locale, t),
      })
    case 'tokensToday':
      return t('cost.tokensToday', { value: number(cost.value) })
    case 'consumption':
      return t('cost.consumption', { value: number(cost.value) })
  }
}
