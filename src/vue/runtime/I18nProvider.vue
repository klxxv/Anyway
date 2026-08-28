<script setup lang="ts">
import { provideI18n } from "./i18n";

provideI18n();
</script>

<template>
  <slot />
</template>

<style scoped>
:slotted(*) {
  min-width: 0;
}
</style>
