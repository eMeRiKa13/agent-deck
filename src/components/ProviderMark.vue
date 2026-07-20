<script setup lang="ts">
import { computed } from 'vue'
import claudeLogo from '@/assets/ai-logos/claude.svg'
import codexLogo from '@/assets/ai-logos/codex.svg'
import cursorLogo from '@/assets/ai-logos/cursor.svg'
import type { ProviderId } from '@/domain/dashboard'

const { provider, size = 'md' } = defineProps<{
  provider: ProviderId
  size?: 'xs' | 'sm' | 'md' | 'lg'
}>()

const names: Record<ProviderId, string> = {
  codex: 'Codex',
  claude: 'Claude',
  cursor: 'Cursor',
}

const logos: Record<ProviderId, string> = {
  codex: codexLogo,
  claude: claudeLogo,
  cursor: cursorLogo,
}

const sizes = {
  xs: 'size-5 rounded-md',
  sm: 'size-8 rounded-[10px]',
  md: 'size-10 rounded-[12px]',
  lg: 'size-12 rounded-[14px]',
}

const classes = computed(() => [`provider-mark--${provider}`, sizes[size]])
</script>

<template>
  <span class="provider-mark" :class="classes" role="img" :aria-label="names[provider]">
    <img :src="logos[provider]" alt="" draggable="false" />
  </span>
</template>
