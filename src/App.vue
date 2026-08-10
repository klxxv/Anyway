<script setup lang="ts">
import ResearchWorkspaceApp from "./vue/ResearchWorkspaceApp.vue";
import CustomCursor from "./vue/components/CustomCursor.vue";
import I18nProvider from "./vue/runtime/I18nProvider.vue";
import PluginHostProvider from "./vue/runtime/PluginHostProvider.vue";
</script>

<template>
  <PluginHostProvider>
    <I18nProvider>
      <ResearchWorkspaceApp />
      <CustomCursor />
    </I18nProvider>
  </PluginHostProvider>
</template>

<style>
#app {
  min-height: 100vh;
}
</style>
