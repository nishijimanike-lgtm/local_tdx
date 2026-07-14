<script setup lang="ts">
defineProps<{ connected: boolean; text: string }>()
</script>
<template>
  <div class="flex items-center gap-2.5" :class="connected ? 'text-emerald-400' : 'text-rose-400'">
    <span class="relative flex h-2.5 w-2.5">
      <span v-if="connected" class="animate-live-pulse absolute inset-0 rounded-full bg-emerald-400" />
      <span class="relative inline-flex rounded-full h-2.5 w-2.5" :class="connected ? 'bg-emerald-400' : 'bg-rose-400'" />
    </span>
    <span class="text-xs font-mono text-slate-400">{{ text }}</span>
  </div>
</template>
