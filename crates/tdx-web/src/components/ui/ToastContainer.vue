<script setup lang="ts">
import type { Toast } from '../../types'
defineProps<{ toasts: Toast[] }>()
</script>
<template>
  <div class="fixed bottom-6 right-6 z-50 space-y-2">
    <div v-for="t in toasts" :key="t.id"
      class="px-4 py-3 rounded-xl text-sm font-medium shadow-xl backdrop-blur-md border animate-slide-up"
      :class="{
        'bg-emerald-500/20 border-emerald-500/30 text-emerald-300': t.type === 'success',
        'bg-rose-500/20 border-rose-500/30 text-rose-300': t.type === 'error',
        'bg-sky-500/20 border-sky-500/30 text-sky-300': t.type === 'info',
      }">
      {{ t.message }}
    </div>
  </div>
</template>
