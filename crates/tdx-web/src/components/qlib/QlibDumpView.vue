<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
import { qlibApi, type QlibDumpStats, type QlibProgress } from '../../api/qlib'

const loading = ref(false)
const stats = ref<QlibDumpStats | null>(null)
const error = ref('')
const progress = ref<QlibProgress | null>(null)
let pollTimer: ReturnType<typeof setInterval> | null = null

const pct = computed(() => {
  if (!progress.value?.progress) return 0
  const p = progress.value.progress
  return p.total > 0 ? Math.round((p.processed / p.total) * 100) : 0
})

onUnmounted(() => {
  stopPolling()
})

function stopPolling() {
  if (pollTimer !== null) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

function startPolling() {
  stopPolling()
  pollTimer = setInterval(async () => {
    try {
      const p = await qlibApi.progress()
      progress.value = p
      if (!p.running) {
        stopPolling()
        loading.value = false
        if (p.stats) {
          stats.value = p.stats
        } else if (p.error) {
          error.value = p.error
        }
      }
    } catch {
      // ignore polling errors
    }
  }, 500)
}

async function runDump() {
  loading.value = true
  error.value = ''
  stats.value = null
  progress.value = null

  try {
    await qlibApi.dump()
    startPolling()
  } catch (e: any) {
    loading.value = false
    error.value = e.message || String(e)
  }
}

function formatDuration(secs: number): string {
  if (secs < 60) return `${secs.toFixed(1)}s`
  const m = Math.floor(secs / 60)
  const s = (secs % 60).toFixed(0)
  return `${m}m ${s}s`
}
</script>

<template>
  <div class="max-w-4xl mx-auto">
    <div class="flex items-center justify-between mb-6">
      <div>
        <h2 class="text-xl font-semibold text-slate-50">Qlib 数据导出</h2>
        <p class="text-sm text-slate-400 mt-1">
          将通达信本地数据转换为 Qlib 二进制格式，用于量化模型训练、回测与预测
        </p>
      </div>
      <button
        class="px-5 py-2.5 rounded-xl font-medium text-sm transition-all duration-200 flex items-center gap-2"
        :class="loading
          ? 'bg-slate-800 text-slate-400 cursor-not-allowed'
          : 'bg-indigo-600 hover:bg-indigo-500 text-white shadow-lg shadow-indigo-500/25 active:scale-[0.98]'"
        :disabled="loading"
        @click="runDump"
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
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
        {{ loading ? '导出中...' : '开始导出 Qlib 数据' }}
      </button>
    </div>

    <!-- Description card -->
    <div class="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 mb-6">
      <h3 class="text-sm font-medium text-slate-300 mb-3">输出格式说明</h3>
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4 text-xs text-slate-400">
        <div class="space-y-1.5">
          <div class="flex items-center gap-1.5 text-slate-300 font-medium">
            <svg class="w-3.5 h-3.5 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" /></svg>
            calendars/day.txt
          </div>
          <div class="pl-5">交易日历 (YYYY-MM-DD)</div>
        </div>
        <div class="space-y-1.5">
          <div class="flex items-center gap-1.5 text-slate-300 font-medium">
            <svg class="w-3.5 h-3.5 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" /></svg>
            features/{sh|sz|bj}xxxxxx/
          </div>
          <div class="pl-5">8 个 .day.bin 文件 (open/high/low/close/volume/amount/vwap/factor)</div>
        </div>
        <div class="space-y-1.5">
          <div class="flex items-center gap-1.5 text-slate-300 font-medium">
            <svg class="w-3.5 h-3.5 text-violet-400" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" /></svg>
            instruments/all.txt
          </div>
          <div class="pl-5">证券列表及时间区间</div>
        </div>
      </div>
    </div>

    <!-- Error -->
    <div v-if="error" class="rounded-xl border border-red-800 bg-red-950/30 p-4 mb-4">
      <p class="text-sm text-red-400 font-medium mb-1">导出失败</p>
      <p class="text-xs text-red-300 font-mono">{{ error }}</p>
    </div>

    <!-- Progress bar -->
    <div
      v-if="progress"
      class="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 mb-4"
    >
      <div class="flex items-center justify-between mb-2">
        <span class="text-sm text-slate-300 font-medium">
          {{ progress.running ? '导出中...' : progress.error ? '导出失败' : '导出完成' }}
        </span>
        <span v-if="progress.progress" class="text-xs text-slate-400 font-mono tabular-nums">
          {{ progress.progress.processed }} / {{ progress.progress.total }}
        </span>
      </div>

      <!-- Bar -->
      <div class="w-full h-3 rounded-full bg-slate-800 overflow-hidden mb-2">
        <div
          class="h-full rounded-full transition-all duration-300 ease-out"
          :class="progress.running
            ? 'bg-gradient-to-r from-indigo-500 to-violet-500'
            : progress.error
              ? 'bg-red-500'
              : 'bg-emerald-500'"
          :style="{ width: (progress.running ? pct : 100) + '%' }"
        />
      </div>

      <!-- Percentage -->
      <div class="text-xs text-slate-400 text-right mb-2">
        {{ progress.running ? pct : 100 }}%
      </div>

      <!-- Current symbol / message -->
      <div v-if="progress.progress?.current_symbol" class="flex items-center gap-2 text-xs text-slate-500">
        <svg v-if="progress.running" class="animate-spin w-3 h-3 text-indigo-400" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
        <span class="text-slate-400 font-mono">{{ progress.progress.current_symbol }}</span>
        <span v-if="progress.progress.message" class="text-slate-600">
          &mdash; {{ progress.progress.message }}
        </span>
      </div>
    </div>

    <!-- Results -->
    <div v-if="stats" class="rounded-2xl border border-slate-800 bg-slate-900/60 p-6">
      <h3 class="text-sm font-medium text-slate-300 mb-4">导出结果</h3>
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
        <div class="rounded-xl bg-slate-950 p-3 text-center">
          <div class="text-xs text-slate-500 mb-1">总文件数</div>
          <div class="text-lg font-mono font-semibold text-slate-200">{{ stats.total_files }}</div>
        </div>
        <div class="rounded-xl bg-slate-950 p-3 text-center">
          <div class="text-xs text-slate-500 mb-1">成功处理</div>
          <div class="text-lg font-mono font-semibold text-emerald-400">{{ stats.processed }}</div>
        </div>
        <div class="rounded-xl bg-slate-950 p-3 text-center">
          <div class="text-xs text-slate-500 mb-1">跳过</div>
          <div class="text-lg font-mono font-semibold text-amber-400">{{ stats.skipped }}</div>
        </div>
        <div class="rounded-xl bg-slate-950 p-3 text-center">
          <div class="text-xs text-slate-500 mb-1">失败</div>
          <div class="text-lg font-mono font-semibold" :class="stats.failed > 0 ? 'text-red-400' : 'text-slate-400'">{{ stats.failed }}</div>
        </div>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4 text-xs text-slate-400">
        <div>
          <span class="text-slate-500">输出目录: </span>
          <code class="text-indigo-400 bg-slate-950 px-1.5 py-0.5 rounded">{{ stats.output_dir }}</code>
        </div>
        <div>
          <span class="text-slate-500">交易日: </span>
          <span class="text-slate-300">{{ stats.calendar_days }} 天</span>
        </div>
        <div>
          <span class="text-slate-500">耗时: </span>
          <span class="text-slate-300">{{ formatDuration(stats.elapsed_secs) }}</span>
        </div>
      </div>

      <!-- Failures details -->
      <div v-if="stats.failures.length > 0" class="mt-4">
        <div class="text-xs text-red-400 font-medium mb-2">失败详情 ({{ stats.failures.length }} 只):</div>
        <div class="max-h-40 overflow-y-auto rounded-lg bg-slate-950 p-3">
          <div v-for="(f, idx) in stats.failures" :key="idx" class="text-xs text-red-300 font-mono py-0.5">{{ f }}</div>
        </div>
      </div>
    </div>

    <!-- Usage hint -->
    <div class="mt-6 rounded-xl border border-slate-800/50 bg-slate-900/40 p-4">
      <h3 class="text-xs font-medium text-slate-400 mb-2">使用方式</h3>
      <div class="space-y-1 text-xs text-slate-500 font-mono">
        <div class="flex items-center gap-2">
          <span class="text-amber-400">Python:</span>
          <code>qlib.init(provider_uri='{{ stats?.output_dir || './qlib_bin' }}')</code>
        </div>
        <div class="flex items-center gap-2">
          <span class="text-amber-400">WSL:</span>
          <code>cp -r ./qlib_bin ~/.qlib/qlib_data/cn_data/</code>
        </div>
      </div>
    </div>
  </div>
</template>
