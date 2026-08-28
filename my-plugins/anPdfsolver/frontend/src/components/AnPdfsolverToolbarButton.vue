<template>
  <button class="anpdfsolver-toolbar-button" type="button" @click="openPanel">
    <span class="anpdfsolver-toolbar-icon">PDF</span>
    <span>Analyze PDF</span>
  </button>
</template>

<script setup lang="ts">
import type { HostContext } from "../context";
import { useAnalysisStore } from "../store";

defineProps<{ hostContext: HostContext }>();

const { openPanel } = useAnalysisStore();
</script>
