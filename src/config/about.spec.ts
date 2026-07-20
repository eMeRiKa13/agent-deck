import { invoke, isTauri } from '@tauri-apps/api/core'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import mainCapability from '../../src-tauri/capabilities/main.json'
import tauriConfig from '../../src-tauri/tauri.conf.json'
import {
  ALLOWED_EXTERNAL_URLS,
  APP_VERSION,
  EXTERNAL_LINKS,
  openExternalLink,
  resolveExternalLink,
  type ExternalLinkId,
} from '@/config/about'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }))

const mockedIsTauri = vi.mocked(isTauri)
const mockedInvoke = vi.mocked(invoke)

describe('About metadata and external links', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('keeps the displayed version aligned with the Tauri bundle', () => {
    expect(APP_VERSION).toBe(tauriConfig.version)
  })

  it('delegates allow-listed links to the native desktop command', async () => {
    mockedIsTauri.mockReturnValue(true)

    for (const linkId of Object.keys(EXTERNAL_LINKS) as ExternalLinkId[]) {
      await openExternalLink(linkId, 'fr')
    }

    expect(mockedInvoke.mock.calls).toEqual([
      ['open_external_link', { linkId: 'enchanter', locale: 'fr' }],
      ['open_external_link', { linkId: 'github', locale: 'fr' }],
      ['open_external_link', { linkId: 'corsair', locale: 'fr' }],
    ])
  })

  it('resolves the localized CORSAIR page and keeps the other URLs stable', () => {
    expect(resolveExternalLink('corsair', 'en')).toBe(EXTERNAL_LINKS.corsair.en)
    expect(resolveExternalLink('corsair', 'fr')).toBe(EXTERNAL_LINKS.corsair.fr)
    expect(resolveExternalLink('enchanter', 'en')).toBe(EXTERNAL_LINKS.enchanter.en)
    expect(resolveExternalLink('enchanter', 'fr')).toBe(EXTERNAL_LINKS.enchanter.fr)
    expect(resolveExternalLink('github', 'en')).toBe(EXTERNAL_LINKS.github.en)
    expect(resolveExternalLink('github', 'fr')).toBe(EXTERNAL_LINKS.github.fr)
  })

  it('keeps the desktop capability scoped to the five exact URLs', () => {
    const openerPermission = mainCapability.permissions.find(
      permission => typeof permission === 'object' && permission.identifier === 'opener:allow-open-url',
    )

    const allowedUrls = openerPermission && typeof openerPermission !== 'string'
      ? openerPermission.allow.map(({ url }) => url)
      : []

    expect(allowedUrls).toEqual(ALLOWED_EXTERNAL_URLS)
  })

  it('uses a safe browser fallback in the web preview', async () => {
    mockedIsTauri.mockReturnValue(false)
    const openWindow = vi.spyOn(window, 'open').mockImplementation(() => null)

    await openExternalLink('corsair', 'en')

    expect(openWindow).toHaveBeenCalledWith(EXTERNAL_LINKS.corsair.en, '_blank', 'noopener,noreferrer')
    expect(mockedInvoke).not.toHaveBeenCalled()
  })

  it('rejects links outside the fixed allow list', async () => {
    mockedIsTauri.mockReturnValue(true)

    await expect(openExternalLink('unknown' as ExternalLinkId, 'en')).rejects.toThrow('External link is not allowed')
    expect(mockedInvoke).not.toHaveBeenCalled()
  })
})
