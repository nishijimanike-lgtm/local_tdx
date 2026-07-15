<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { fetchFreshness, type FreshnessReport } from '../../api/checker'

const report = ref<FreshnessReport | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const lastChecked = ref<string>('')

async function doCheck() {
  loading.value = true
  error.value = null
  try {
    report.value = await fetchFreshness()
    lastChecked.value = new Date().toLocaleTimeString()
  } catch (e: any) {
    error.value = e.message || '检查失败'
  } finally {
    loading.value = false
  }
}

const statusClass = (status: string) => {
  switch (status) {
    case 'up_to_date': return 'text-emerald-400 bg-emerald-500/10'
    case 'behind': return 'text-amber-400 bg-amber-500/10'
    case 'no_data': return 'text-red-400 bg-red-500/10'
    case 'server_unreachable': return 'text-slate-400 bg-slate-500/10'
    default: return 'text-slate-400 bg-slate-500/10'
  }
}

const statusLabel = (status: string) => {
  switch (status) {
    case 'up_to_date': return '已是最新'
    case 'behind': return '需要更新'
    case 'no_data': return '无本地数据'
    case 'server_unreachable': return '服务器不可达'
    default: return status
  }
}

const marketName = (m: string) => {
  switch (m) {
    case 'sh': return '上海 (SH)'
    case 'sz': return '深圳 (SZ)'
    case 'bj': return '北交所 (BJ)'
    default: return m
  }
}

const bannerClass = computed(() => {
  if (!report.value) return ''
  if (!report.value.server_reachable) return 'border-amber-600 bg-amber-950/30'
  if (report.value.needs_update) return 'border-red-600 bg-red-950/20'
  return 'border-emerald-600 bg-emerald-950/20'
})

const bannerIcon = computed(() => {
  if (!report.value) return ''
  if (!report.value.server_reachable) return '⚠️'
  if (report.value.needs_update) return '🔴'
  return '✅'
})

const bannerText = computed(() => {
  if (!report.value) return '正在检查...'
  if (!report.value.server_reachable) return '无法连接 TDX 行情服务器，请确认网络状态'
  if (report.value.needs_update) return '本地数据存在滞后，建议立即更新'
  return '本地数据与服务器一致，无需更新'
})

const pct = (n: number, total: number) => {
  if (total === 0) return '0%'
  return ((n / total) * 100).toFixed(1) + '%'
}

onMounted(() => {
  doCheck()
})
</script>

<template>
  <div class="max-w-6xl mx-auto space-y-6">
    <!-- Page header -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-bold text-slate-50">数据更新检查</h2>
        <p class="text-sm text-slate-400 mt-1">
          对比 TDX 行情服务器与本地 vipdoc 数据，确定是否需要更新
        </p>
      </div>
      <button
        @click="doCheck"
        :disabled="loading"
        class="px-5 py-2.5 rounded-lg font-medium text-sm transition-all
               bg-indigo-600 hover:bg-indigo-500 text-white
               disabled:opacity-50 disabled:cursor-not-allowed
               flex items-center gap-2 shadow-lg shadow-indigo-500/20"
      >
        <svg
          v-if="loading"
          class="animate-spin w-4 h-4"
          fill="none" viewBox="0 0 24 24"
        >
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
        <svg v-else class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
            d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
        {{ loading ? '检查中...' : '重新检查' }}
      </button>
    </div>

    <!-- Error -->
    <div v-if="error" class="p-4 rounded-xl border border-red-600 bg-red-950/20 text-red-400 text-sm">
      {{ error }}
    </div>

    <!-- Banner -->
    <div
      v-if="report"
      :class="['p-4 rounded-xl border flex items-start gap-3 text-sm', bannerClass]"
    >
      <span class="text-xl shrink-0 mt-0.5">{{ bannerIcon }}</span>
      <div class="flex-1">
        <div :class="[
          'font-semibold',
          report.server_reachable && !report.needs_update ? 'text-emerald-400' :
          report.server_reachable ? 'text-red-400' : 'text-amber-400'
        ]">
          {{ bannerText }}
        </div>
        <div class="text-slate-400 mt-1 text-xs">
          检查时间: {{ report.checked_at }}
        </div>
      </div>
    </div>

    <!-- Summary -->
    <div v-if="report" class="grid grid-cols-4 gap-4">
      <div class="bg-slate-900 rounded-xl p-4 border border-slate-800">
        <div class="text-2xl font-bold text-slate-50">{{ report.summary.total_stocks }}</div>
        <div class="text-xs text-slate-400 mt-1">总股票数</div>
      </div>
      <div class="bg-slate-900 rounded-xl p-4 border border-slate-800">
        <div class="text-2xl font-bold text-emerald-400">{{ report.summary.up_to_date }}</div>
        <div class="text-xs text-slate-400 mt-1">已是最新</div>
        <div class="text-xs text-emerald-500/60 mt-0.5">{{ pct(report.summary.up_to_date, report.summary.total_stocks) }}</div>
      </div>
      <div class="bg-slate-900 rounded-xl p-4 border border-slate-800">
        <div class="text-2xl font-bold text-amber-400">{{ report.summary.behind }}</div>
        <div class="text-xs text-slate-400 mt-1">数据滞后</div>
        <div class="text-xs text-amber-500/60 mt-0.5">{{ pct(report.summary.behind, report.summary.total_stocks) }}</div>
      </div>
      <div class="bg-slate-900 rounded-xl p-4 border border-slate-800">
        <div class="text-2xl font-bold text-red-400">{{ report.summary.missing }}</div>
        <div class="text-xs text-slate-400 mt-1">缺失数据</div>
        <div class="text-xs text-red-500/60 mt-0.5">{{ pct(report.summary.missing, report.summary.total_stocks) }}</div>
      </div>
    </div>

    <!-- Per-market detail -->
    <div v-if="report" class="space-y-4">
      <h3 class="text-sm font-semibold text-slate-400 uppercase tracking-wider">分市场详情</h3>

      <div
        v-for="m in report.markets"
        :key="m.market"
        class="bg-slate-900 rounded-xl border border-slate-800 overflow-hidden"
      >
        <!-- Market header -->
        <div class="px-5 py-4 flex items-center justify-between border-b border-slate-800">
          <div>
            <span class="text-sm font-semibold text-slate-200">{{ marketName(m.market) }}</span>
            <span class="text-xs text-slate-500 ml-2">{{ m.total_stocks }} 只股票</span>
          </div>
          <span :class="['px-3 py-1 rounded-full text-xs font-medium', statusClass(m.status)]">
            {{ statusLabel(m.status) }}
          </span>
        </div>

        <!-- Server vs local comparison -->
        <div class="px-5 py-3 grid grid-cols-2 gap-4 bg-slate-950/50">
          <div>
            <div class="text-xs text-slate-500 mb-0.5">服务器最新日期</div>
            <div class="text-sm font-mono text-slate-200">
              {{ m.server_latest_date || '—' }}
            </div>
          </div>
          <div>
            <div class="text-xs text-slate-500 mb-0.5">本地最新日期</div>
            <div class="text-sm font-mono text-slate-200">
              {{ m.local_latest_date || '—' }}
              <span v-if="m.days_behind !== null" class="ml-2 text-xs"
                    :class="m.days_behind > 0 ? 'text-amber-400' : 'text-emerald-400'">
                {{ m.days_behind > 0 ? `滞后 ${m.days_behind} 天` : '同步' }}
              </span>
            </div>
          </div>
        </div>

        <!-- Stock counts bar -->
        <div class="px-5 py-3">
          <div class="flex items-center gap-1 text-xs text-slate-400 mb-2">
            <span class="flex items-center gap-1">
              <span class="w-2 h-2 rounded-sm bg-emerald-500 inline-block"></span>
              已同步 {{ m.up_to_date_stocks }}
            </span>
            <span class="flex items-center gap-1 ml-3">
              <span class="w-2 h-2 rounded-sm bg-amber-500 inline-block"></span>
              滞后 {{ m.behind_stocks }}
            </span>
            <span class="flex items-center gap-1 ml-3">
              <span class="w-2 h-2 rounded-sm bg-red-500 inline-block"></span>
              缺失 {{ m.missing_stocks }}
            </span>
          </div>
          <!-- Progress bar -->
          <div class="w-full h-2 rounded-full bg-slate-800 overflow-hidden flex">
            <div
              v-if="m.up_to_date_stocks > 0"
              class="h-full bg-emerald-500 transition-all duration-500"
              :style="{ width: pct(m.up_to_date_stocks, m.total_stocks) }"
            />
            <div
              v-if="m.behind_stocks > 0"
              class="h-full bg-amber-500 transition-all duration-500"
              :style="{ width: pct(m.behind_stocks, m.total_stocks) }"
            />
            <div
              v-if="m.missing_stocks > 0"
              class="h-full bg-red-500 transition-all duration-500"
              :style="{ width: pct(m.missing_stocks, m.total_stocks) }"
            />
          </div>
        </div>
      </div>
    </div>

    <!-- Empty state -->
    <div
      v-if="!report && !loading && !error"
      class="text-center py-16 text-slate-500"
    >
      <svg class="w-12 h-12 mx-auto mb-3 opacity-30" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
          d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
      </svg>
      <p>点击 "重新检查" 开始扫描</p>
    </div>
  </div>
</template>
