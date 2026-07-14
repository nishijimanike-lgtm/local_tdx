<script setup lang="ts">
import { computed } from 'vue'
import { useDashboardStore } from '../../stores/dashboard'
const { data: d, parquet: pq } = useDashboardStore()

function timeAgo(iso: string): { text: string; cls: string } {
  if (!iso) return { text: '—', cls: 'text-slate-500' }
  const diff = Date.now() - new Date(iso).getTime()
  const mins = Math.floor(diff / 60000)
  const hrs = Math.floor(mins / 60)
  const days = Math.floor(hrs / 24)
  if (days > 1) return { text: `${days} 天前`, cls: 'text-rose-400' }
  if (hrs > 1) return { text: `${hrs} 小时前`, cls: 'text-amber-400' }
  if (mins > 10) return { text: `${mins} 分钟前`, cls: 'text-amber-400' }
  return { text: '刚刚', cls: 'text-emerald-400' }
}

const dailyFreshness = computed(() => timeAgo(d.last_daily_update))
const adjFreshness = computed(() => timeAgo(d.last_adj_factor_update))

const statCards = [
  { label: '交易日', value: d.counts.open_days, sub: d.calendar_source, accent: 'emerald', icon: 'M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z' },
  { label: 'XDXR 事件', value: d.counts.xdxr_events.toLocaleString(), sub: adjFreshness.value.text, subCls: adjFreshness.value.cls, accent: 'amber', icon: 'M13 17h8m0 0V9m0 8l-8-8-4 4-6-6' },
  { label: '因子覆盖', value: d.counts.adj_factor_symbols.toLocaleString(), sub: `${d.adj_factor_tier} 等级`, accent: 'sky', icon: 'M7 12l3-3 3 3 4-4M8 21l4-4 4 4M3 4h18M4 4h16v12a1 1 0 01-1 1H5a1 1 0 01-1-1V4z' },
  { label: 'Parquet 文件', value: pq.total_files, sub: `${pq.total_size_mb} MB`, accent: 'violet', icon: 'M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4' },
]
</script>

<template>
  <div class="space-y-6">

    <!-- Data Freshness Banner -->
    <div class="flex items-center gap-4 px-5 py-3 rounded-xl border text-sm"
      :class="dailyFreshness.cls === 'text-emerald-400' ? 'bg-emerald-500/5 border-emerald-500/15' : dailyFreshness.cls === 'text-amber-400' ? 'bg-amber-500/5 border-amber-500/15' : 'bg-rose-500/5 border-rose-500/15'">
      <span class="relative flex h-2.5 w-2.5">
        <span class="animate-live-pulse absolute inset-0 rounded-full" :class="dailyFreshness.cls === 'text-emerald-400' ? 'bg-emerald-400' : dailyFreshness.cls === 'text-amber-400' ? 'bg-amber-400' : 'bg-rose-400'" />
        <span class="relative inline-flex rounded-full h-2.5 w-2.5" :class="dailyFreshness.cls === 'text-emerald-400' ? 'bg-emerald-400' : dailyFreshness.cls === 'text-amber-400' ? 'bg-amber-400' : 'bg-rose-400'" />
      </span>
      <span class="text-slate-300">日线数据</span>
      <span :class="dailyFreshness.cls">{{ dailyFreshness.text }}</span>
      <span class="text-slate-600">·</span>
      <span class="text-slate-300">复权因子</span>
      <span :class="adjFreshness.cls">{{ adjFreshness.text }}</span>
      <span v-if="d.daily_bar_range.start" class="ml-auto text-xs text-slate-500 font-mono">
        {{ d.daily_bar_range.start }} → {{ d.daily_bar_range.end }}
      </span>
    </div>

    <!-- Stat Cards -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
      <div v-for="card in statCards" :key="card.label"
        class="glass-panel rounded-xl p-5 border border-slate-800/50 hover:border-slate-700/70 transition-colors">
        <div class="flex items-center gap-3 mb-3">
          <div class="p-2 rounded-lg" :class="`bg-${card.accent}-500/10 border border-${card.accent}-500/20 text-${card.accent}-400`">
            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" :d="card.icon" /></svg>
          </div>
          <span class="text-xs text-slate-500">{{ card.label }}</span>
        </div>
        <div class="text-2xl font-bold tracking-tight text-slate-100">{{ card.value }}</div>
        <div class="text-xs mt-2" :class="card.subCls || 'text-slate-500'">{{ card.sub }}</div>
      </div>
    </div>

    <!-- Parquet Detail -->
    <div v-if="pq.exists && Object.keys(pq.markets).length > 0" class="glass-panel rounded-xl border border-slate-800/50 p-6">
      <h3 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-4">Parquet 存储</h3>
      <div class="flex gap-2">
        <div v-for="(v, k) in pq.markets" :key="k"
          class="flex-1 text-center py-3 px-4 rounded-lg border"
          :class="k === 'sh' ? 'bg-rose-500/5 border-rose-500/10' : k === 'sz' ? 'bg-emerald-500/5 border-emerald-500/10' : 'bg-sky-500/5 border-sky-500/10'">
          <div class="text-xs font-mono mb-1" :class="k === 'sh' ? 'text-rose-400' : k === 'sz' ? 'text-emerald-400' : 'text-sky-400'">{{ k.toUpperCase() }}</div>
          <div class="text-lg font-bold text-slate-200">{{ v.files }}</div>
          <div class="text-xs text-slate-500">{{ v.size_mb }} MB</div>
        </div>
      </div>
    </div>
  </div>
</template>
