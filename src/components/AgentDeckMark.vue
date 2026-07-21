<script setup lang="ts">
import { useId } from 'vue'
import agentDeckMark from '@/assets/branding/agent-deck-mark.png'

const structureFilterId = `agent-deck-structure-${useId()}`
</script>

<template>
  <svg
    class="agent-deck-mark"
    viewBox="0 0 1024 561"
    role="presentation"
    focusable="false"
    aria-hidden="true"
  >
    <defs>
      <filter
        :id="structureFilterId"
        x="-2%"
        y="-2%"
        width="104%"
        height="104%"
        color-interpolation-filters="sRGB"
      >
        <!-- Isolate the near-white structure without altering the orange and green marks. -->
        <feColorMatrix
          in="SourceGraphic"
          type="matrix"
          values="
            0 0 0 0 1
            0 0 0 0 1
            0 0 0 0 1
            0.2126 0.7152 0.0722 0 0
          "
          result="luminance-mask"
        />
        <feComponentTransfer in="luminance-mask" result="white-mask">
          <feFuncA type="discrete" tableValues="0 0 0 0 0 0 0 0 0 1" />
        </feComponentTransfer>
        <feComposite in="white-mask" in2="SourceAlpha" operator="in" result="structure-mask" />
        <feFlood flood-color="currentColor" result="theme-structure-color" />
        <feComposite
          in="theme-structure-color"
          in2="structure-mask"
          operator="in"
          result="theme-structure"
        />
        <feMerge>
          <feMergeNode in="SourceGraphic" />
          <feMergeNode in="theme-structure" />
        </feMerge>
      </filter>
    </defs>

    <image
      :href="agentDeckMark"
      width="1024"
      height="561"
      :filter="`url(#${structureFilterId})`"
      preserveAspectRatio="xMidYMid meet"
    />
  </svg>
</template>
