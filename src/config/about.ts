import { invoke, isTauri } from '@tauri-apps/api/core'
import packageJson from '../../package.json'

export const APP_VERSION = packageJson.version

export const EXTERNAL_LINKS = {
  enchanter: {
    en: 'https://enchanter.gg/en/',
    fr: 'https://enchanter.gg/fr/',
  },
  github: {
    en: 'https://github.com/eMeRiKa13/agent-deck',
    fr: 'https://github.com/eMeRiKa13/agent-deck',
  },
  corsair: {
    en: 'https://www.corsair.com/us/en/p/monitors/cc-9011311-ww/corsair-xeneon-edge-14-5-lcd-touchscreen-white-cc-9011311-ww',
    fr: 'https://www.corsair.com/fr/fr/p/monitors/cc-9011311-ww/corsair-xeneon-edge-14-5-lcd-touchscreen-white-cc-9011311-ww',
  },
} as const

export type ExternalLinkId = keyof typeof EXTERNAL_LINKS
export type ExternalLinkLocale = keyof (typeof EXTERNAL_LINKS)[ExternalLinkId]

export const ALLOWED_EXTERNAL_URLS = [...new Set(
  Object.values(EXTERNAL_LINKS).flatMap(urls => Object.values(urls)),
)]

export function resolveExternalLink(linkId: ExternalLinkId, locale: ExternalLinkLocale) {
  const urls = EXTERNAL_LINKS[linkId]
  if (!urls) throw new Error(`External link is not allowed: ${linkId}`)
  return urls[locale]
}

export async function openExternalLink(linkId: ExternalLinkId, locale: ExternalLinkLocale) {
  const url = resolveExternalLink(linkId, locale)

  if (isTauri()) {
    await invoke('open_external_link', { linkId, locale })
    return
  }

  window.open(url, '_blank', 'noopener,noreferrer')
}
