<script setup lang="ts">
defineProps<{ connected: boolean; text: string }>()
</script>
<template>
  <div class="flex items-center gap-2 px-3 py-1.5 rounded-lg border text-xs font-mono"
    :class="connected ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-400' : 'bg-rose-500/10 border-rose-500/20 text-rose-400'">
    <span class="relative flex h-2 w-2"><span class="animate-ping absolute inline-flex h-full w-full rounded-full opacity-75" :class="connected ? 'bg-emerald-400' : 'bg-rose-400'" /><span class="relative inline-flex rounded-full h-2 w-2" :class="connected ? 'bg-emerald-400' : 'bg-rose-400'" /></span>
    {{ text }}
  </div>
</template>
