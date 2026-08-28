<script setup lang="ts">
import { providePluginHost } from "./plugin-host";

providePluginHost();
</script>

<template>
  <slot />
</template>

<style scoped>
:slotted(*) {
  min-width: 0;
}
</style>
