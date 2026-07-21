import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from '@/App.vue'
import { DemoProviderAdapter } from '@/data/demo'
import { i18n, LOCALE_STORAGE_KEY, setAppLocale } from '@/i18n'

const mountedWrappers: Array<{ unmount: () => void }> = []

function mountApp(locale: 'fr' | 'en' = 'fr') {
  setAppLocale(locale, false)
  const wrapper = mount(App, {
    attachTo: document.body,
    global: {
      plugins: [i18n],
      stubs: { teleport: true },
    },
  })
  mountedWrappers.push(wrapper)
  return wrapper
}

describe('Agent dashboard', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.className = ''
    document.documentElement.lang = ''
  })

  afterEach(() => {
    mountedWrappers.splice(0).forEach((wrapper) => wrapper.unmount())
    document.body.innerHTML = ''
    vi.useRealTimers()
    vi.restoreAllMocks()
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

  it('keeps the selected detail inside the visible search results', async () => {
    const wrapper = mountApp()
    await flushPromises()

    await wrapper.get('input[type="search"]').setValue('Corriger l’indexation')

    expect(wrapper.findAll('[data-session-row]')).toHaveLength(1)
    expect(wrapper.get('[data-session-row]').classes()).not.toContain('session-row--selected')
    await wrapper.get('[data-session-row]').trigger('click')
    expect(wrapper.get('[data-session-row]').classes()).toContain('session-row--selected')
    expect(wrapper.get('.conversation-header h1').text()).toBe('Corriger l’indexation des conversations')

    await wrapper.get('input[type="search"]').setValue('aucun résultat possible')

    expect(wrapper.find('.conversation-panel').exists()).toBe(false)
    expect(wrapper.text()).toContain('Aucune session ne correspond aux critères actuels.')
  })

  it('selects the first visible session when a refresh removes the current one', async () => {
    const firstSnapshot = await new DemoProviderAdapter().getSnapshot()
    const nextSnapshot = {
      ...firstSnapshot,
      sessions: firstSnapshot.sessions.filter((session) => session.id !== 'cursor-index'),
      refreshedAtMs: firstSnapshot.refreshedAtMs + 1_000,
    }
    vi.spyOn(DemoProviderAdapter.prototype, 'getSnapshot')
      .mockResolvedValueOnce(firstSnapshot)
      .mockResolvedValueOnce(nextSnapshot)

    const wrapper = mountApp()
    await flushPromises()

    const cursorSession = wrapper
      .findAll('[data-session-row]')
      .find((row) => row.text().includes('Corriger l’indexation des conversations'))
    await cursorSession?.trigger('click')
    await wrapper.get('.conversation-panel__close').trigger('click')
    await wrapper.get('[aria-label="Rafraîchir"]').trigger('click')
    await flushPromises()

    expect(wrapper.get('[data-session-row]').classes()).not.toContain('session-row--selected')
    await wrapper.get('[data-session-row]').trigger('click')
    expect(wrapper.get('[data-session-row]').classes()).toContain('session-row--selected')
    expect(wrapper.get('.conversation-header h1').text()).toBe('Construire le tableau de bord agents')
  })

  it('keeps the previous snapshot visible when a refresh fails', async () => {
    const initialSnapshot = await new DemoProviderAdapter().getSnapshot()
    vi.spyOn(DemoProviderAdapter.prototype, 'getSnapshot')
      .mockResolvedValueOnce(initialSnapshot)
      .mockRejectedValueOnce(new Error('base locale verrouillée'))

    const wrapper = mountApp()
    await flushPromises()
    await wrapper.get('[aria-label="Rafraîchir"]').trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Codex')
    expect(wrapper.get('.refresh-error').attributes('role')).toBe('status')
    expect(wrapper.get('.refresh-error').text()).toContain('base locale verrouillée')
  })

  it('polls sessions without reloading or clearing the quotas', async () => {
    vi.useFakeTimers()
    const demoSnapshot = await new DemoProviderAdapter().getSnapshot()
    const initialSnapshot = {
      ...demoSnapshot,
      providers: demoSnapshot.providers.map((provider) => provider.id === 'codex'
        ? { ...provider, error: { key: 'errors.providerPartial', params: { provider: 'Codex' } } }
        : provider),
    }
    const getSnapshot = vi.spyOn(DemoProviderAdapter.prototype, 'getSnapshot')
      .mockResolvedValueOnce(initialSnapshot)
      .mockResolvedValueOnce({
        ...initialSnapshot,
        sessions: initialSnapshot.sessions.slice(1),
        quotas: [],
        refreshedAtMs: initialSnapshot.refreshedAtMs + 15_000,
      })

    const wrapper = mountApp()
    await flushPromises()
    expect(wrapper.findAll('.quota-card')).toHaveLength(3)

    await vi.advanceTimersByTimeAsync(15_000)
    await flushPromises()

    expect(getSnapshot.mock.calls[0]?.[0]).toEqual({ includeQuotas: true })
    expect(getSnapshot.mock.calls[1]?.[0]).toEqual({ includeQuotas: false })
    expect(wrapper.findAll('.quota-card')).toHaveLength(3)
    expect(wrapper.text()).not.toContain('Construire le tableau de bord agents')
    const codexProvider = wrapper
      .findAll('.provider-nav-item')
      .find((provider) => provider.text().includes('Codex'))
    expect(codexProvider?.attributes('title')).toContain('Certaines données Codex sont indisponibles')
  })

  it('keeps a quota warning when no previous quota value exists', async () => {
    vi.useFakeTimers()
    const demoSnapshot = await new DemoProviderAdapter().getSnapshot()
    const initialSnapshot = {
      ...demoSnapshot,
      providers: demoSnapshot.providers.map((provider) => provider.id === 'codex'
        ? { ...provider, error: { key: 'errors.providerPartial', params: { provider: 'Codex' } } }
        : provider),
      quotas: demoSnapshot.quotas.filter((quota) => quota.provider !== 'codex'),
    }
    vi.spyOn(DemoProviderAdapter.prototype, 'getSnapshot')
      .mockResolvedValueOnce(initialSnapshot)
      .mockResolvedValueOnce({
        ...demoSnapshot,
        quotas: [],
        refreshedAtMs: demoSnapshot.refreshedAtMs + 15_000,
      })

    const wrapper = mountApp()
    await flushPromises()
    await vi.advanceTimersByTimeAsync(15_000)
    await flushPromises()

    const codexProvider = wrapper
      .findAll('.provider-nav-item')
      .find((provider) => provider.text().includes('Codex'))
    expect(codexProvider?.attributes('title')).toContain('Certaines données Codex sont indisponibles')
  })

  it('uses the resolved system theme when leaving automatic mode', async () => {
    vi.spyOn(window, 'matchMedia').mockImplementation((query) => ({
      matches: query === '(prefers-color-scheme: dark)' || query === '(max-width: 1799px)',
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(() => true),
    }))

    const wrapper = mountApp()
    await flushPromises()
    expect(document.documentElement.classList.contains('dark')).toBe(true)

    await wrapper.get('[aria-label="Changer de thème"]').trigger('click')
    await flushPromises()

    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(localStorage.getItem('vueuse-color-scheme')).toBe('light')
  })

  it('describes completed and attention states without presenting them as waiting', async () => {
    const wrapper = mountApp()
    await flushPromises()

    const completedSession = wrapper
      .findAll('[data-session-row]')
      .find((row) => row.text().includes('Préparer la revue de la pull request'))
    await completedSession?.trigger('click')
    expect(wrapper.get('.session-overview__status').text()).toContain('Session terminée')
    expect(wrapper.get('.session-overview__status').text()).toContain('Aucune action n’est requise')

    await wrapper.get('.conversation-panel__close').trigger('click')
    const attentionSession = wrapper
      .findAll('[data-session-row]')
      .find((row) => row.text().includes('Corriger l’indexation des conversations'))
    await attentionSession?.trigger('click')
    expect(wrapper.get('.session-overview__status').text()).toContain('Validation requise')
    expect(wrapper.get('.session-overview__status').text()).toContain('Une action de votre part est nécessaire')
  })

  it('traps focus in the compact detail and restores it on close', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.find('.session-row--selected').exists()).toBe(false)
    const trigger = wrapper
      .findAll('[data-session-row]')
      .find((row) => row.text().includes('Corriger l’indexation des conversations'))
    ;(trigger?.element as HTMLButtonElement | undefined)?.focus()
    await trigger?.trigger('click')
    await flushPromises()

    const closeButton = wrapper.get('.conversation-panel__close')
    expect(trigger?.classes()).toContain('session-row--selected')
    expect(document.activeElement).toBe(closeButton.element)
    expect(wrapper.get('.session-column').attributes('inert')).toBeDefined()
    expect(wrapper.get('.usage-panel').attributes('inert')).toBeDefined()
    expect(wrapper.get('.dashboard-topbar').attributes('inert')).toBeDefined()

    await closeButton.trigger('keydown', { key: 'Tab' })
    expect(document.activeElement).toBe(closeButton.element)

    await closeButton.trigger('click')
    await flushPromises()
    expect(wrapper.find('.session-row--selected').exists()).toBe(false)
    expect(document.activeElement).toBe(trigger?.element)
  })

  it('keeps the selected session visible when the desktop detail is always displayed', async () => {
    vi.spyOn(window, 'matchMedia').mockImplementation((query) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(() => true),
    }))

    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.find('.conversation-panel').exists()).toBe(true)
    expect(wrapper.findAll('.session-row--selected')).toHaveLength(1)
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

  it('shows provider-backed metrics without inventing unavailable token usage', async () => {
    const wrapper = mountApp()
    await flushPromises()

    const cursorSession = wrapper
      .findAll('[data-session-row]')
      .find((row) => row.text().includes('Corriger l’indexation des conversations'))
    await cursorSession?.trigger('click')

    expect(wrapper.get('.session-metrics').text()).toContain('Contexte utilisé')
    expect(wrapper.get('.token-breakdown--empty').text()).toContain('Indisponible')
    expect(wrapper.get('.token-breakdown--empty').text()).toContain('ne stocke pas cette mesure')

    await wrapper.get('.conversation-panel__close').trigger('click')
    const codexSession = wrapper
      .findAll('[data-session-row]')
      .find((row) => row.text().includes('Construire le tableau de bord agents'))
    await codexSession?.trigger('click')

    expect(wrapper.get('.session-metrics').text()).toContain('Total tokens')
    expect(wrapper.find('.token-breakdown--empty').exists()).toBe(false)
    expect(wrapper.findAll('.activity-group').length).toBeGreaterThan(0)
  })

  it('disables unfinished navigation and enables session filtering', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.get('[data-nav-item="activity"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-nav-item="usage"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-session-filter]').attributes('disabled')).toBeUndefined()
    expect(wrapper.get('[data-session-filter]').attributes('aria-expanded')).toBe('false')
    expect(wrapper.get('[data-session-filter]').attributes('aria-haspopup')).toBe('menu')
    expect(wrapper.text()).toContain('Sessions récentes')
    expect(wrapper.text()).toContain('Actualisation auto · 15 s')
  })

  it('filters sessions by one or several statuses and resets with all statuses', async () => {
    const wrapper = mountApp()
    await flushPromises()

    const trigger = wrapper.get('[data-session-filter]')
    await trigger.trigger('click')
    await flushPromises()

    expect(trigger.attributes('aria-expanded')).toBe('true')
    expect(wrapper.get('[data-status-filter-menu]').attributes('role')).toBe('menu')
    expect(wrapper.findAll('[data-status-filter]')).toHaveLength(5)
    expect(wrapper.get('[data-status-filter="all"]').attributes('aria-checked')).toBe('true')

    await wrapper.get('[data-status-filter="waiting"]').trigger('click')
    expect(wrapper.findAll('[data-session-row]')).toHaveLength(1)
    expect(wrapper.get('[data-session-row]').text()).toContain('Revoir le moteur de synchronisation')
    expect(wrapper.get('[data-session-row]').classes()).not.toContain('session-row--selected')
    expect(trigger.text()).toContain('1')
    expect(trigger.attributes('aria-label')).toBe('1 statut sélectionné')

    await wrapper.get('[data-status-filter="attention"]').trigger('click')
    expect(wrapper.findAll('[data-session-row]')).toHaveLength(2)
    expect(wrapper.get('[data-status-filter="waiting"]').attributes('aria-checked')).toBe('true')
    expect(wrapper.get('[data-status-filter="attention"]').attributes('aria-checked')).toBe('true')

    await wrapper.get('[data-status-filter="all"]').trigger('click')
    expect(wrapper.findAll('[data-session-row]')).toHaveLength(6)
    expect(wrapper.get('.session-column .panel-heading__count').text()).toBe('6')
    expect(trigger.attributes('aria-label')).toBe('Filtrer par statut')
  })

  it('intersects status filters with search and provider filters', async () => {
    const wrapper = mountApp()
    await flushPromises()

    await wrapper.get('[data-session-filter]').trigger('click')
    await wrapper.get('[data-status-filter="waiting"]').trigger('click')
    await wrapper.get('input[type="search"]').setValue('Revoir le moteur')

    expect(wrapper.findAll('[data-session-row]')).toHaveLength(1)

    const cursorProvider = wrapper
      .findAll('.provider-nav-item')
      .find((provider) => provider.text().includes('Cursor'))
    await cursorProvider?.trigger('click')

    expect(wrapper.findAll('[data-session-row]')).toHaveLength(0)
    expect(wrapper.text()).toContain('Aucune session ne correspond aux critères actuels.')
    expect(wrapper.find('.conversation-panel').exists()).toBe(false)
  })

  it('closes the status menu with Escape or an outside click and restores focus on Escape', async () => {
    const wrapper = mountApp()
    await flushPromises()

    const trigger = wrapper.get('[data-session-filter]')
    await trigger.trigger('click')
    await flushPromises()
    expect(document.activeElement).toBe(wrapper.get('[data-status-filter="all"]').element)

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(wrapper.find('[data-status-filter-menu]').exists()).toBe(false)
    expect(document.activeElement).toBe(trigger.element)

    await trigger.trigger('click')
    await flushPromises()
    await wrapper.get('.dashboard-topbar').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-status-filter-menu]').exists()).toBe(false)
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
    expect(panel?.querySelector('[data-external-link="enchanter"] img.about-panel__link-icon'))
      .not.toBeNull()

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
    await wrapper.get('[data-session-row]').trigger('click')
    expect(wrapper.find('.conversation-panel').exists()).toBe(true)
    expect(wrapper.find('.dashboard-workspace').exists()).toBe(true)
    expect(wrapper.findAll('.quota-card')).toHaveLength(3)
    expect(wrapper.findAll('.provider-mark img').length).toBeGreaterThan(3)
  })

  it('expands a single usage window without changing multi-window quotas', async () => {
    const wrapper = mountApp()
    await flushPromises()

    const quotaCards = wrapper.findAll('.quota-card')
    const codexCard = quotaCards.find((card) => card.text().includes('Codex'))
    const claudeCard = quotaCards.find((card) => card.text().includes('Claude'))
    const cursorCard = quotaCards.find((card) => card.text().includes('Cursor'))

    expect(codexCard?.get('.quota-card__windows').classes())
      .toContain('quota-card__windows--single')
    expect(claudeCard?.get('.quota-card__windows').classes())
      .not.toContain('quota-card__windows--single')
    expect(cursorCard?.get('.quota-card__windows').classes())
      .not.toContain('quota-card__windows--single')
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

    await wrapper.get('[data-session-filter]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-status-filter-menu]').attributes('aria-label')).toBe('Filter sessions by status')
    expect(wrapper.get('[data-status-filter="all"]').text()).toContain('All statuses')
    expect(wrapper.get('[data-status-filter="waiting"]').text()).toContain('Waiting')

    await wrapper.get('[data-about-trigger]').trigger('click')
    await flushPromises()
    expect(document.body.querySelector('[data-about-panel]')?.textContent)
      .toContain('Designed for the CORSAIR XENEON EDGE')
  })
})
