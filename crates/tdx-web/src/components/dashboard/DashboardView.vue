<script setup lang="ts">
import { useDashboardStore } from '../../stores/dashboard'
const { data: dashboard, parquet: parquetStats } = useDashboardStore()
</script>
<template>
  <div class="space-y-8">
    <!-- Stats Cards Row -->
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6 gap-6">
      <div class="bg-slate-900/40 backdrop-blur-sm border border-slate-800/50 rounded-xl p-6 hover:border-indigo-500/30 transition-all">
        <div class="flex items-center justify-between mb-4">
          <span class="text-xs text-slate-500">复权等级</span>
          <div class="p-2 rounded-lg bg-indigo-500/10 border border-indigo-500/20 text-indigo-400"><svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" /></svg></div>
        </div>
        <div class="text-3xl font-display font-bold text-indigo-400">{{ dashboard.adj_factor_tier }}</div>
        <div class="flex items-center gap-1.5 text-xs text-slate-400 mt-3">{{ dashboard.last_probe_at || '-' }}</div>
      </div>
      <div class="bg-slate-900/40 backdrop-blur-sm border border-slate-800/50 rounded-xl p-6 hover:border-emerald-500/30 transition-all">
        <div class="flex items-center justify-between mb-4"><span class="text-xs text-slate-500">交易日</span><div class="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400"><svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" /></svg></div></div>
        <div class="text-3xl font-display font-bold text-emerald-400">{{ dashboard.counts.open_days }}</div>
        <div class="text-xs text-slate-400 mt-3">{{ dashboard.calendar_source }}</div>
      </div>
      <div class="bg-slate-900/40 backdrop-blur-sm border border-slate-800/50 rounded-xl p-6 hover:border-amber-500/30 transition-all">
        <div class="flex items-center justify-between mb-4"><span class="text-xs text-slate-500">XDXR 事件</span><div class="p-2 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-400"><svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 17h8m0 0V9m0 8l-8-8-4 4-6-6" /></svg></div></div>
        <div class="text-3xl font-display font-bold text-amber-400">{{ dashboard.counts.xdxr_events.toLocaleString() }}</div>
        <div class="text-xs text-slate-400 mt-3">{{ dashboard.last_adj_factor_update || '-' }}</div>
      </div>
      <div class="bg-slate-900/40 backdrop-blur-sm border border-slate-800/50 rounded-xl p-6 hover:border-sky-500/30 transition-all">
        <div class="flex items-center justify-between mb-4"><span class="text-xs text-slate-500">复权因子覆盖</span><div class="p-2 rounded-lg bg-sky-500/10 border border-sky-500/20 text-sky-400"><svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 12l3-3 3 3 4-4M8 21l4-4 4 4M3 4h18M4 4h16v12a1 1 0 01-1 1H5a1 1 0 01-1-1V4z" /></svg></div></div>
        <div class="text-3xl font-display font-bold text-sky-400">{{ dashboard.counts.adj_factor_symbols.toLocaleString() }}</div>
        <div class="text-xs text-slate-400 mt-3">支股票</div>
      </div>
    </div>

    <!-- Parquet Stats -->
    <div v-if="parquetStats.exists" class="bg-slate-900/40 backdrop-blur-sm border border-slate-800/50 rounded-xl p-6">
      <h2 class="text-sm font-semibold text-slate-300 mb-4">Parquet 存储统计</h2>
      <div class="grid grid-cols-3 gap-6">
        <div v-for="(v, k) in parquetStats.markets" :key="k" class="text-center p-4 bg-slate-800/30 rounded-lg border border-slate-700/50">
          <div class="text-sm text-slate-400 mb-1">{{ k.toUpperCase() }}</div>
          <div class="text-2xl font-bold text-slate-200">{{ v.files }}</div>
          <div class="text-xs text-slate-500">{{ v.size_mb }} MB</div>
        </div>
      </div>
    </div>

    <!-- Daily Bar Range -->
    <div v-if="dashboard.daily_bar_range.start" class="bg-slate-900/40 backdrop-blur-sm border border-slate-800/50 rounded-xl p-6">
      <h2 class="text-sm font-semibold text-slate-300 mb-2">日线数据范围</h2>
      <p class="text-slate-400 font-mono text-sm">{{ dashboard.daily_bar_range.start }} → {{ dashboard.daily_bar_range.end }}</p>
    </div>
  </div>
</template>
