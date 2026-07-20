import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import App from '@/App.vue'

describe('Agent dashboard', () => {
  it('renders the three providers and selects a session', async () => {
    const wrapper = mount(App)
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
    const wrapper = mount(App)
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
    const wrapper = mount(App)
    await flushPromises()

    expect(wrapper.get('[data-nav-item="Activité"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-nav-item="Utilisation"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-session-filter]').attributes('disabled')).toBeDefined()
    expect(wrapper.text()).toContain('Sessions récentes')
    expect(wrapper.text()).toContain('Actualisation auto · 15 s')
  })

  it('keeps usage visible in the responsive workspace', async () => {
    const wrapper = mount(App)
    await flushPromises()

    expect(wrapper.find('.usage-panel').exists()).toBe(true)
    expect(wrapper.find('.conversation-panel').exists()).toBe(true)
    expect(wrapper.find('.dashboard-workspace').exists()).toBe(true)
    expect(wrapper.findAll('.quota-card')).toHaveLength(3)
    expect(wrapper.findAll('.provider-mark img').length).toBeGreaterThan(3)
  })
})
