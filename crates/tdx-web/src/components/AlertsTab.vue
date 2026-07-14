<script setup lang="ts">
import { inject, type Ref } from 'vue'
import type { Alert } from '../types'
const alerts = inject<Ref<Alert[]>>('alertsList')!
const acknowledge = inject<(id: number) => Promise<void>>('acknowledgeAlert')!
</script>
<template>
  <div class="space-y-4">
    <div v-if="alerts.length === 0" class="text-center py-20 text-slate-500"><svg class="w-12 h-12 mx-auto mb-4 opacity-20" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>暂无告警</div>
    <div v-for="a in alerts" :key="a.id" class="p-4 rounded-xl border"
      :class="a.acknowledged ? 'bg-slate-900/30 border-slate-800/30 opacity-60' : a.level === 'error' ? 'bg-rose-500/5 border-rose-500/10' : 'bg-slate-900/30 border-slate-800/30'">
      <div class="flex items-start justify-between gap-4">
        <div>
          <div class="flex items-center gap-2 mb-1">
            <span class="text-xs px-2 py-0.5 rounded font-mono" :class="a.level === 'error' ? 'bg-rose-500/20 text-rose-400' : 'bg-amber-500/20 text-amber-400'">{{ a.level }}</span>
            <span class="text-xs text-slate-500">{{ a.category }}</span>
          </div>
          <p class="text-sm text-slate-200 mb-1">{{ a.message }}</p>
          <p v-if="a.detail" class="text-xs text-slate-500">{{ a.detail }}</p>
        </div>
        <button v-if="!a.acknowledged" @click="acknowledge(a.id)" class="shrink-0 px-3 py-1 text-xs rounded-lg bg-indigo-500/10 border border-indigo-500/20 text-indigo-400 hover:bg-indigo-500/20">确认</button>
      </div>
    </div>
  </div>
</template>
