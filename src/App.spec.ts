import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import App from '@/App.vue'
import { i18n, LOCALE_STORAGE_KEY, setAppLocale } from '@/i18n'

function mountApp(locale: 'fr' | 'en' = 'fr') {
  setAppLocale(locale, false)
  return mount(App, { attachTo: document.body, global: { plugins: [i18n] } })
}

describe('Agent dashboard', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.lang = ''
  })

  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('renders the three providers and selects a session', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.text()).toContain('Codex')
    expect(wrapper.text()).toContain('Claude')
    expect(wrapper.text()).toContain('Cursor')

    const cursorSession = wrapper
      .findAll('[data-session-row]')
      .find((row) => row.text().includes('Corriger l’indexation des conversations'))
    await cursorSession?.trigger('click')

    expect(wrapper.find('.conversation-header h1').text()).toBe('Corriger l’indexation des conversations')
  })

  it('keeps the dashboard read-only', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.text()).not.toContain('Nouvelle session')
    expect(wrapper.text()).not.toContain('Réglages')
    expect(wrapper.text()).not.toContain('Reprendre')
    expect(wrapper.text()).not.toContain('Arrêter')
    expect(wrapper.find('[aria-label="Copier le lien"]').exists()).toBe(false)
    expect(wrapper.find('[aria-label="Plus d’options"]').exists()).toBe(false)
    expect(wrapper.find('.quota-card__header button').exists()).toBe(false)
    expect(wrapper.find('.profile-button').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('Rythme soutenu')
  })

  it('disables unfinished navigation and filtering controls', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.get('[data-nav-item="activity"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-nav-item="usage"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-session-filter]').attributes('disabled')).toBeDefined()
    expect(wrapper.text()).toContain('Sessions récentes')
    expect(wrapper.text()).toContain('Actualisation auto · 15 s')
  })

  it('opens the bilingual read-only About panel and restores focus when it closes', async () => {
    const wrapper = mountApp()
    await flushPromises()

    const trigger = wrapper.get('[data-about-trigger]')
    ;(trigger.element as HTMLButtonElement).focus()
    await trigger.trigger('click')
    await flushPromises()

    const panel = document.body.querySelector<HTMLElement>('[data-about-panel]')
    expect(panel?.getAttribute('role')).toBe('dialog')
    expect(panel?.textContent).toContain('Pensé pour le CORSAIR XENEON EDGE')
    expect(panel?.textContent).toContain('Antoine Frankart')
    expect(panel?.textContent).toContain('Freelance Product Engineer')
    expect(panel?.textContent).toContain('Version 0.1.0')
    expect(panel?.querySelectorAll('[data-external-link]')).toHaveLength(3)

    document.body.querySelector<HTMLButtonElement>('[data-about-close]')?.click()
    await flushPromises()

    expect(document.body.querySelector('[data-about-panel]')).toBeNull()
    expect(document.activeElement).toBe(trigger.element)
  })

  it('closes About from the backdrop and Escape key', async () => {
    const wrapper = mountApp()
    await flushPromises()

    const trigger = wrapper.get('[data-about-trigger]')
    await trigger.trigger('click')
    await flushPromises()
    document.body.querySelector<HTMLButtonElement>('.about-backdrop')?.click()
    await flushPromises()
    expect(document.body.querySelector('[data-about-panel]')).toBeNull()

    await trigger.trigger('click')
    await flushPromises()
    document.body.querySelector<HTMLButtonElement>('[data-about-close]')?.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    )
    await flushPromises()
    expect(document.body.querySelector('[data-about-panel]')).toBeNull()
  })

  it('keeps usage visible in the responsive workspace', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.find('.usage-panel').exists()).toBe(true)
    expect(wrapper.find('.conversation-panel').exists()).toBe(true)
    expect(wrapper.find('.dashboard-workspace').exists()).toBe(true)
    expect(wrapper.findAll('.quota-card')).toHaveLength(3)
    expect(wrapper.findAll('.provider-mark img').length).toBeGreaterThan(3)
  })

  it('switches the complete interface to English and persists the choice', async () => {
    const wrapper = mountApp()
    await flushPromises()

    const selector = wrapper.get('[data-locale-selector]')
    expect(selector.element.tagName).toBe('SELECT')
    await selector.setValue('en')

    expect(wrapper.text()).toContain('Control center')
    expect(wrapper.text()).toContain('Recent sessions')
    expect(wrapper.text()).toContain('Build the agent dashboard')
    expect(wrapper.text()).not.toContain('Centre de contrôle')
    expect(document.documentElement.lang).toBe('en')
    expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('en')
    expect(selector.attributes('aria-label')).toBe('Change language')

    await wrapper.get('[data-about-trigger]').trigger('click')
    await flushPromises()
    expect(document.body.querySelector('[data-about-panel]')?.textContent)
      .toContain('Designed for the CORSAIR XENEON EDGE')
  })
})
