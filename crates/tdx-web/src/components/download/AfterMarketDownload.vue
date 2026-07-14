<script setup lang="ts">
import { ref, computed, watch, onUnmounted, type Ref } from 'vue'
import { useTasksStore } from '../../stores/tasks'

const tasks = useTasksStore()
const showToast = (m: string, t: 'success' | 'error' | 'info' = 'success') =>
  (window as any).__toast?.(m, t)

// Download options
const dateStart = ref('')
const dateEnd = ref('')
const downloadDailyIncr = ref(true)
const downloadDailyFull = ref(false)
const downloadXdxr = ref(true)
const downloadAdjFactor = ref(false)
const downloadCalendar = ref(false)
const downloadScan = ref(false)

const dataSource = ref<'remote' | 'local'>('remote')

// Init dates to last 7 days
const today = new Date()
const weekAgo = new Date(today.getTime() - 7 * 86400000)
dateStart.value = weekAgo.toISOString().split('T')[0]
dateEnd.value = today.toISOString().split('T')[0]

// Download queue management
const downloadQueue = ref<string[]>([])
const currentIndex = ref(-1)
const isDownloading = ref(false)

interface DownloadItem {
  id: string
  label: string
  checked: Ref<boolean>
}

const items = computed<DownloadItem[]>(() => {
  if (dataSource.value === 'local') {
    return [
      { id: 'local-import', label: '本地日线数据校验与导入', checked: ref(true) },
      { id: 'xdxr-sync', label: '除权除息事件 (XDXR)', checked: downloadXdxr },
      { id: 'calendar-sync', label: '交易日历', checked: downloadCalendar },
      { id: 'daily_bars', label: '数据完整性扫描', checked: downloadScan },
    ]
  }
  return [
    { id: 'daily-increment', label: '日线 — 增量更新 (仅补齐缺失)', checked: downloadDailyIncr },
    { id: 'daily-full', label: '日线 — 全量下载 (覆盖全部历史)', checked: downloadDailyFull },
    { id: 'xdxr-sync', label: '除权除息事件 (XDXR)', checked: downloadXdxr },
    { id: 'adj-factor-sync', label: '复权因子 (L3)', checked: downloadAdjFactor },
    { id: 'calendar-sync', label: '交易日历', checked: downloadCalendar },
    { id: 'daily_bars', label: '数据完整性扫描', checked: downloadScan },
  ]
})
const checkedCount = computed(() => items.value.filter(i => i.checked.value).length)

// Progress display
const taskLabelMap: Record<string, string> = {
  calendar_sync: '交易日历构建',
  daily_bar_full: '全量日线更新',
  daily_bar_update: '增量日线更新',
  daily_bar_gap_fill: '空缺填补',
  xdxr_sync: 'XDXR 同步',
  adj_factor_update: '复权因子重构',
  daily_bar_scan: '完整性扫描',
  local_import: '本地数据导入',
}

function currentTaskLabel() {
  return taskLabelMap[tasks.progress.task_type] || tasks.progress.task_type || '—'
}
function percent() {
  return tasks.progress.total > 0 ? Math.round((tasks.progress.done / tasks.progress.total) * 100) : 0
}
function running() { return !tasks.progress.finished }
function paused() { return tasks.progress.paused }

const visible = ref(false)
let hideTimer: ReturnType<typeof setTimeout> | null = null

watch(() => tasks.progress.finished, (finished) => {
  if (!finished) {
    visible.value = true
    isDownloading.value = true
    if (hideTimer) { clearTimeout(hideTimer); hideTimer = null }
  } else if (tasks.progress.task_id > 0 && currentIndex.value >= 0) {
    // Task completed, start next in queue
    currentIndex.value++
    if (currentIndex.value < downloadQueue.value.length) {
      const next = downloadQueue.value[currentIndex.value]
      setTimeout(() => tasks.trigger(next), 500)
    } else {
      // All done
      isDownloading.value = false
      currentIndex.value = -1
      downloadQueue.value = []
      showToast('盘后数据下载完成', 'success')
      hideTimer = setTimeout(() => { visible.value = false }, 5000)
    }
  }
})

function startDownload() {
  const selected = items.value.filter(i => i.checked.value).map(i => i.id)
  if (selected.length === 0) {
    showToast('请至少选择一项数据', 'error')
    return
  }
  downloadQueue.value = selected
  currentIndex.value = 0
  isDownloading.value = true
  tasks.trigger(selected[0])
  showToast(`开始下载 ${selected.length} 项数据`, 'info')
}

function stopDownload() {
  tasks.control('abort')
  isDownloading.value = false
  currentIndex.value = -1
  downloadQueue.value = []
  showToast('已中止下载')
}

onUnmounted(() => { if (hideTimer) clearTimeout(hideTimer) })
</script>

<template>
  <div class="space-y-6">

    <!-- Options Panel -->
    <div class="glass-panel border border-slate-800/50 rounded-xl overflow-hidden">
      <!-- Header -->
      <div class="px-6 py-4 border-b border-slate-800/50 flex items-center gap-3">
        <div class="p-1.5 rounded-lg bg-indigo-500/10 border border-indigo-500/20 text-indigo-400">
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
          </svg>
        </div>
        <span class="text-sm font-semibold text-slate-200">盘后数据下载</span>
        <span v-if="isDownloading" class="text-xs px-2 py-0.5 rounded-full bg-indigo-500/20 text-indigo-400 border border-indigo-500/30">下载中</span>
      </div>

      <!-- Data Source -->
      <div class="px-6 py-3 border-b border-slate-800/30 flex items-center gap-4">
        <span class="text-xs text-slate-500">数据源</span>
        <label class="flex items-center gap-2 cursor-pointer"><input type="radio" v-model="dataSource" value="remote" class="text-indigo-500" /><span class="text-sm" :class="dataSource === 'remote' ? 'text-slate-200' : 'text-slate-500'">远程下载</span></label>
        <label class="flex items-center gap-2 cursor-pointer"><input type="radio" v-model="dataSource" value="local" class="text-indigo-500" /><span class="text-sm" :class="dataSource === 'local' ? 'text-slate-200' : 'text-slate-500'">本地转换</span></label>
      </div>

      <!-- Date Range -->
      <div class="px-6 py-4 border-b border-slate-800/30 grid grid-cols-1 md:grid-cols-2 gap-4">
        <div>
          <label class="text-xs text-slate-500 block mb-1.5">起始日期</label>
          <input type="date" v-model="dateStart"
            class="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500/50" />
        </div>
        <div>
          <label class="text-xs text-slate-500 block mb-1.5">结束日期</label>
          <input type="date" v-model="dateEnd"
            class="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500/50" />
        </div>
      </div>

      <!-- Checkboxes -->
      <div class="px-6 py-4 space-y-3">
        <label class="text-xs text-slate-500 block">下载数据项 ({{ checkedCount }} 项选中)</label>
        <label v-for="item in items" :key="item.id"
          class="flex items-center gap-3 px-3 py-2.5 rounded-lg border cursor-pointer transition-all"
          :class="item.checked.value ? 'bg-indigo-500/5 border-indigo-500/20' : 'bg-slate-800/20 border-slate-700/50 hover:border-slate-600/50'">
          <input type="checkbox" v-model="item.checked.value"
            class="w-4 h-4 rounded border-slate-600 bg-slate-800 text-indigo-500 focus:ring-indigo-500/30" />
          <span class="text-sm text-slate-300">{{ item.label }}</span>
        </label>
      </div>

      <!-- Info text -->
      <div class="px-6 py-3 bg-slate-950/30 border-t border-slate-800/30">
        <p class="text-xs text-slate-600 leading-relaxed">
          交易日 15:45 后方可下载当天沪深京数据。日线数据覆盖本地原有数据，用于选股、报表分析和复权计算。网络较慢时建议减少品种或缩短时间段。
        </p>
      </div>
    </div>

    <!-- Download Button -->
    <div class="flex gap-3">
      <button v-if="!isDownloading" @click="startDownload"
        class="flex-1 py-3 bg-indigo-600 hover:bg-indigo-500 text-white rounded-xl text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        :disabled="checkedCount === 0">
        <svg class="w-4 h-4 inline mr-2 -mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
        开始下载 ({{ checkedCount }} 项)
      </button>
      <button v-else @click="stopDownload"
        class="flex-1 py-3 bg-rose-600/90 hover:bg-rose-500 text-white rounded-xl text-sm font-medium transition-colors">
        <svg class="w-4 h-4 inline mr-2 -mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
        取消下载
      </button>
    </div>

    <!-- Queue Progress -->
    <div v-if="isDownloading && downloadQueue.length > 0" class="bg-slate-900/40 border border-slate-800/50 rounded-xl p-4">
      <div class="flex items-center gap-2 mb-2 text-xs text-slate-500 font-mono">
        <span>{{ currentIndex + 1 }} / {{ downloadQueue.length }}</span>
        <div class="flex-1 h-px bg-slate-700/50" />
      </div>
      <div class="flex flex-wrap gap-1.5">
        <div v-for="(q, qi) in downloadQueue" :key="q"
          class="text-xs px-2 py-1 rounded"
          :class="qi < currentIndex ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20' :
                  qi === currentIndex ? 'bg-indigo-500/10 text-indigo-400 border border-indigo-500/20' :
                  'bg-slate-800/30 text-slate-600 border border-slate-700/20'">
          {{ items.find(i => i.id === q)?.label || q }}
        </div>
      </div>
    </div>

    <!-- Progress Bar -->
    <div v-if="visible && running()" class="bg-slate-900/40 border border-slate-800/50 rounded-xl p-6">
      <div class="flex items-center justify-between mb-4">
        <div>
          <span class="text-xs text-slate-500 font-mono mr-2">{{ currentTaskLabel() }}</span>
          <h3 class="text-sm font-semibold inline" :class="paused() ? 'text-amber-400' : 'text-indigo-400'">
            {{ paused() ? '⏸ 已暂停' : '🔄 下载中' }}
          </h3>
        </div>
        <span class="text-xs font-mono text-slate-500">{{ tasks.progress.done }} / {{ tasks.progress.total }}</span>
      </div>

      <div class="w-full bg-slate-800 rounded-full h-3 mb-3 overflow-hidden">
        <div class="h-full rounded-full transition-all duration-500"
          :class="paused() ? 'bg-amber-500 animate-pulse' : 'bg-indigo-500'"
          :style="{ width: percent() + '%' }" />
      </div>

      <p class="text-xs text-slate-400 mb-4">{{ tasks.progress.message }}</p>

      <div class="flex gap-3">
        <button v-if="!paused()" @click="tasks.control('pause')"
          class="px-4 py-2 text-xs rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-400 hover:bg-amber-500/20">
          暂停
        </button>
        <button v-else @click="tasks.control('resume')"
          class="px-4 py-2 text-xs rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 hover:bg-emerald-500/20">
          恢复
        </button>
        <button @click="stopDownload"
          class="px-4 py-2 text-xs rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 hover:bg-rose-500/20">
          中止
        </button>
      </div>
    </div>

    <!-- Completed summary -->
    <div v-if="visible && tasks.progress.finished && !running()" class="bg-emerald-500/5 border border-emerald-500/20 rounded-xl p-6 text-center">
      <svg class="w-10 h-10 mx-auto mb-3 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
        <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      <p class="text-sm font-semibold text-emerald-400">全部下载完成</p>
      <p class="text-xs text-slate-500 mt-1">{{ tasks.progress.done }} 项已同步, {{ tasks.progress.failed }} 项失败</p>
    </div>
  </div>
</template>
