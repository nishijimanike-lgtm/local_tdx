<script setup lang="ts">
import { inject, ref, onMounted, computed, type Ref } from 'vue'
import type { CalendarDay } from '../types'
const list = inject<Ref<CalendarDay[]>>('calendarList')!
const fetchCalendar = inject<(start: string, end: string) => Promise<void>>('fetchCalendar')!

const start = ref(new Date().getFullYear() + '-01-01')
const end = ref(new Date().getFullYear() + '-12-31')

function query() { fetchCalendar(start.value, end.value) }
onMounted(() => query())

const openDays = computed(() => list.value.filter(d => d.is_open).length)
</script>
<template>
  <div class="space-y-6">
    <div class="flex items-center gap-4">
      <input type="date" v-model="start" class="bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200" />
      <span class="text-slate-500">→</span>
      <input type="date" v-model="end" class="bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200" />
      <button @click="query" class="px-4 py-2 bg-indigo-500/10 border border-indigo-500/20 text-indigo-400 rounded-lg text-sm hover:bg-indigo-500/20">查询</button>
    </div>
    <div class="text-sm text-slate-400">开盘日: {{ openDays }} / {{ list.length }}</div>
    <div class="bg-slate-900/40 border border-slate-800/50 rounded-xl overflow-hidden max-h-96 overflow-y-auto">
      <table class="w-full text-sm">
        <thead><tr class="text-slate-500 text-xs border-b border-slate-800/50"><th class="p-3 text-left">日期</th><th class="p-3 text-left">开盘</th><th class="p-3 text-left">来源</th></tr></thead>
        <tbody>
          <tr v-for="d in list.slice(-200)" :key="d.trade_date" class="border-b border-slate-800/20" :class="d.is_open ? 'text-slate-300' : 'text-slate-600'">
            <td class="p-3 font-mono">{{ d.trade_date }}</td>
            <td class="p-3">{{ d.is_open ? '📈' : '—' }}</td>
            <td class="p-3 text-xs">{{ d.source }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
