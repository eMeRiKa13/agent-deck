import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import UsagePanel from '@/components/UsagePanel.vue'
import { DemoProviderAdapter } from '@/data/demo'
import { i18n, setAppLocale } from '@/i18n'
import { formatResetCountdown } from '@/i18n/format'

const wrappers: Array<{ unmount(): void }> = []
afterEach(() => { wrappers.splice(0).forEach((wrapper) => wrapper.unmount()); vi.useRealTimers() })

async function mountUsage(locale: 'fr' | 'en' = 'fr') {
  setAppLocale(locale, false)
  const snapshot = await new DemoProviderAdapter().getSnapshot()
  const wrapper = mount(UsagePanel, { props: { quotas: snapshot.quotas, providers: snapshot.providers }, global: { plugins: [i18n] } })
  wrappers.push(wrapper)
  return wrapper
}

describe('usage priorities and countdowns', () => {
  it('keeps main limits visible and puts model quotas and credits in read-only disclosures', async () => {
    const wrapper = await mountUsage()
    const codex = wrapper.get('[data-provider="codex"]')
    expect(codex.get('.quota-card__windows').text()).toContain('Hebdomadaire')
    expect(codex.get('.quota-card__windows').text()).toContain('utilisés')
    expect(codex.get('.quota-details').attributes('open')).toBeUndefined()
    expect(codex.get('.quota-details__cost').text()).toContain('crédits')
    const claude = wrapper.get('[data-provider="claude"]')
    expect(claude.get('.quota-card__windows').findAll('.quota-window')).toHaveLength(2)
    expect(claude.get('.quota-card__windows').text()).not.toContain('Fable')
    expect(claude.get('.quota-details').text()).toContain('Fable')
    const cursor = wrapper.get('[data-provider="cursor"]')
    expect(cursor.get('.quota-card__windows').text()).toContain('Modèles Cursor')
    expect(cursor.get('.quota-card__windows').text()).toContain('Autres modèles')
    expect(wrapper.findAll('button')).toHaveLength(0)
  })

  it('updates countdowns and removes expired offers even without a provider poll', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2030-01-01T12:00:00Z'))
    const wrapper = await mountUsage()
    const quotas = wrapper.props('quotas').map((quota) => ({ ...quota, resetOffers: [{ count: 1, expiresAtMs: Date.now() + 30_000 }] }))
    await wrapper.setProps({ quotas })
    expect(wrapper.findAll('.quota-reset-offer')).toHaveLength(3)
    expect(wrapper.get('[data-provider="claude"] .quota-window__reset').text()).toContain('48 min')
    await vi.advanceTimersByTimeAsync(60_000)
    expect(wrapper.findAll('.quota-reset-offer')).toHaveLength(0)
    expect(wrapper.get('[data-provider="claude"] .quota-window__reset').text()).toContain('47 min')
  })

  it('renders English usage and never invents offers when none are reported', async () => {
    const wrapper = await mountUsage('en')
    expect(wrapper.get('.quota-card__windows').text()).toContain('used')
    expect(wrapper.get('.quota-reset-offer').text()).toContain('free reset')
    await wrapper.setProps({ quotas: wrapper.props('quotas').map((quota) => ({ ...quota, resetOffers: undefined })) })
    expect(wrapper.find('.quota-reset-offer').exists()).toBe(false)
    expect(formatResetCountdown(1000, 2000, i18n.global.t)).toBe('awaiting reset')
    expect(formatResetCountdown(2000 + (5 * 24 + 19) * 3_600_000, 2000, i18n.global.t)).toBe('resets in 5d 19h')
  })
})
