<script setup lang="ts">
import agentDeckMarkDark from "@/assets/branding/agent-deck-mark.png";
import agentDeckMarkLight from "@/assets/branding/agent-deck-mark-light.png";
</script>

<template>
  <span class="agent-deck-mark pl-1" aria-hidden="true">
    <img
      class="agent-deck-mark__image agent-deck-mark__image--light"
      :src="agentDeckMarkLight"
      alt=""
      draggable="false"
    />
    <img
      class="agent-deck-mark__image agent-deck-mark__image--dark"
      :src="agentDeckMarkDark"
      alt=""
      draggable="false"
    />
  </span>
</template>
