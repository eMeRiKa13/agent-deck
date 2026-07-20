import { describe, expect, it } from 'vitest'
import { resolveInitialLocale } from '@/i18n'

describe('locale resolution', () => {
  it('uses French for a French system language', () => {
    expect(resolveInitialLocale(null, 'fr-FR')).toBe('fr')
    expect(resolveInitialLocale(null, 'fr-CA')).toBe('fr')
  })

  it('falls back to English for English and unsupported languages', () => {
    expect(resolveInitialLocale(null, 'en-GB')).toBe('en')
    expect(resolveInitialLocale(null, 'de-DE')).toBe('en')
  })

  it('gives a stored preference priority over the system language', () => {
    expect(resolveInitialLocale('en', 'fr-FR')).toBe('en')
    expect(resolveInitialLocale('fr', 'en-US')).toBe('fr')
  })

  it('ignores an invalid stored value', () => {
    expect(resolveInitialLocale('es', 'fr-FR')).toBe('fr')
  })
})
