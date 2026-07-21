<script setup lang="ts">
import {
  ArrowUpRightIcon,
  MonitorIcon,
  ShieldCheckIcon,
  XIcon,
} from '@lucide/vue'
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import agentDeckMark from '@/assets/branding/agent-deck-mark.png'
import enchanterFavicon from '@/assets/branding/enchanter-favicon.ico'
import githubInvertocatBlack from '@/assets/branding/github-invertocat-black.svg'
import githubInvertocatWhite from '@/assets/branding/github-invertocat-white.svg'
import { APP_VERSION, openExternalLink, type ExternalLinkId, type ExternalLinkLocale } from '@/config/about'

const props = defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const { t, locale } = useI18n({ useScope: 'global' })
const panel = ref<HTMLElement | null>(null)
const closeButton = ref<HTMLButtonElement | null>(null)
let previousActiveElement: HTMLElement | null = null

const focusableSelector = [
  'button:not([disabled])',
  '[href]',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

watch(
  () => props.open,
  async (isOpen) => {
    if (isOpen) {
      previousActiveElement = document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null
      await nextTick()
      closeButton.value?.focus()
      return
    }

    const elementToRestore = previousActiveElement
    previousActiveElement = null
    await nextTick()
    if (elementToRestore?.isConnected) elementToRestore.focus()
  },
  { flush: 'sync' },
)

onBeforeUnmount(() => {
  if (previousActiveElement?.isConnected) previousActiveElement.focus()
})

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    emit('close')
    return
  }

  if (event.key !== 'Tab' || !panel.value) return

  const focusable = [...panel.value.querySelectorAll<HTMLElement>(focusableSelector)]
    .filter((element) => !element.hasAttribute('disabled'))
  if (!focusable.length) return

  const first = focusable[0]
  const last = focusable[focusable.length - 1]

  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last?.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first?.focus()
  }
}

async function handleExternalLink(linkId: ExternalLinkId) {
  await openExternalLink(linkId, locale.value as ExternalLinkLocale)
}
</script>

<template>
  <Teleport to="body">
    <div v-if="props.open" class="about-overlay" @keydown="handleKeydown">
      <button
        type="button"
        class="about-backdrop"
        :aria-label="t('about.close')"
        @click="emit('close')"
      />

      <aside
        ref="panel"
        class="about-panel"
        data-about-panel
        role="dialog"
        aria-modal="true"
        :aria-labelledby="'about-title'"
        :aria-describedby="'about-summary'"
      >
        <header class="about-panel__header">
          <div class="about-panel__identity">
            <img :src="agentDeckMark" alt="" draggable="false" />
            <div>
              <h2 id="about-title">{{ t('app.name') }}</h2>
              <span>{{ t('about.version', { version: APP_VERSION }) }}</span>
            </div>
          </div>
          <button
            ref="closeButton"
            type="button"
            class="about-panel__close"
            data-about-close
            :aria-label="t('about.close')"
            @click="emit('close')"
          >
            <XIcon :size="21" />
          </button>
        </header>

        <div class="about-panel__body">
          <p id="about-summary" class="about-panel__summary">{{ t('about.summary') }}</p>

          <section class="about-panel__display">
            <div class="about-panel__section-icon" aria-hidden="true">
              <MonitorIcon :size="20" />
            </div>
            <div>
              <h3>{{ t('about.display.title') }}</h3>
              <p>{{ t('about.display.body') }}</p>
              <button type="button" class="about-panel__text-link" data-external-link="corsair" @click="handleExternalLink('corsair')">
                {{ t('about.display.link') }}
                <ArrowUpRightIcon :size="15" />
              </button>
              <small>{{ t('about.display.disclaimer') }}</small>
            </div>
          </section>

          <section class="about-panel__privacy">
            <ShieldCheckIcon :size="20" aria-hidden="true" />
            <div>
              <h3>{{ t('about.privacy.title') }}</h3>
              <p>{{ t('about.privacy.body') }}</p>
            </div>
          </section>

          <section class="about-panel__credits">
            <p class="about-panel__eyebrow">{{ t('about.credits.eyebrow') }}</p>
            <h3>Antoine Frankart</h3>
            <p>{{ t('about.credits.role') }}</p>

            <div class="about-panel__links">
              <button type="button" data-external-link="enchanter" @click="handleExternalLink('enchanter')">
                <img class="about-panel__link-icon" :src="enchanterFavicon" alt="" aria-hidden="true">
                {{ t('about.credits.website') }}
                <ArrowUpRightIcon :size="14" />
              </button>
              <button type="button" data-external-link="github" @click="handleExternalLink('github')">
                <img
                  class="about-panel__link-icon about-panel__link-icon--github-light"
                  :src="githubInvertocatBlack"
                  alt=""
                  aria-hidden="true"
                >
                <img
                  class="about-panel__link-icon about-panel__link-icon--github-dark"
                  :src="githubInvertocatWhite"
                  alt=""
                  aria-hidden="true"
                >
                {{ t('about.credits.github') }}
                <ArrowUpRightIcon :size="14" />
              </button>
            </div>
          </section>
        </div>

        <footer class="about-panel__footer">© 2026 Antoine Frankart</footer>
      </aside>
    </div>
  </Teleport>
</template>
