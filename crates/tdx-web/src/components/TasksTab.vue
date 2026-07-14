<script setup lang="ts">
import { inject, ref, watch, onUnmounted, type Ref } from 'vue'
import type { TaskProgress, TaskLog } from '../types'

const p = inject<Ref<TaskProgress>>('activeTaskProgress')!
const taskList = inject<Ref<TaskLog[]>>('taskList')!
const triggerTask = inject<(a: string) => Promise<void>>('triggerTask')!
const controlTask = inject<(a: string) => Promise<void>>('controlTask')!

const taskTypeMap: Record<string, string> = {
  calendar_sync: '交易日历构建',
  daily_bar_full: '全量日线更新',
  daily_bar_update: '增量日线更新',
  daily_bar_gap_fill: '空缺填补',
  xdxr_sync: 'XDXR 同步',
  adj_factor_update: '复权因子重构',
  daily_bar_scan: '完整性扫描',
  xdxr_scan: 'XDXR 扫描',
  adj_factor_scan: '因子扫描',
}

const tasks = [
  { name: '交易日历构建', action: 'calendar-sync', desc: '根据基准指数生成交易日历' },
  { name: '增量日线更新', action: 'daily-increment', desc: '补齐本地证券最新日线数据' },
  { name: '盘后空缺填补', action: 'daily-gap-fill', desc: '填充缺失的日线区间' },
  { name: '除权除息同步', action: 'xdxr-sync', desc: '同步深沪A股XDXR事件' },
  { name: 'L3 复权因子重构', action: 'adj-factor-sync', desc: '推导全部复权因子' },
  { name: '完整性扫描', action: 'daily_bars', desc: '校验日线数据完整性' },
]

// Derive display properties from the injected ref
function running() { return !p.value.finished }
function paused() { return p.value.paused }
function taskLabel() { return taskTypeMap[p.value.task_type] || p.value.task_type || '未知任务' }
function percent() { return p.value.total > 0 ? Math.round((p.value.done / p.value.total) * 100) : 0 }

const visible = ref(false)
let hideTimer: ReturnType<typeof setTimeout> | null = null

watch(() => p.value.finished, (finished) => {
  if (!finished) {
    visible.value = true
    if (hideTimer) { clearTimeout(hideTimer); hideTimer = null }
  } else if (p.value.task_id > 0) {
    visible.value = true
    hideTimer = setTimeout(() => { visible.value = false }, 5000)
  }
}, { immediate: true })

onUnmounted(() => { if (hideTimer) clearTimeout(hideTimer) })

function doTrigger(action: string) { triggerTask(action) }
function doControl(action: string) { controlTask(action) }
</script>

<template>
  <div class="space-y-8">

    <!-- Task Trigger Buttons -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
      <button v-for="t in tasks" :key="t.action" @click="doTrigger(t.action)" :disabled="running()"
        class="text-left p-5 bg-slate-900/40 border border-slate-800/50 rounded-xl hover:border-indigo-500/30 disabled:opacity-50 disabled:cursor-not-allowed transition-all">
        <div class="text-sm font-semibold text-slate-200 mb-2">{{ t.name }}</div>
        <div class="text-xs text-slate-500">{{ t.desc }}</div>
      </button>
    </div>

    <!-- Progress Panel -->
    <div v-if="visible" class="bg-slate-900/40 border border-slate-800/50 rounded-xl p-6">
      <div class="flex items-center justify-between mb-4">
        <div>
          <span class="text-xs text-slate-500 font-mono mr-2">{{ taskLabel() }}</span>
          <h3 class="text-sm font-semibold inline" :class="paused() ? 'text-amber-400' : p.finished ? 'text-emerald-400' : 'text-indigo-400'">
            {{ paused() ? '⏸ 已暂停' : p.finished ? '✅ 已完成' : '🔄 运行中' }}
          </h3>
        </div>
        <span class="text-xs font-mono text-slate-500">{{ p.done }} / {{ p.total }}</span>
      </div>

      <div class="w-full bg-slate-800 rounded-full h-3 mb-3 overflow-hidden">
        <div class="h-full rounded-full transition-all duration-500"
          :class="paused() ? 'bg-amber-500 animate-pulse' : p.finished ? 'bg-emerald-500' : 'bg-indigo-500'"
          :style="{ width: percent() + '%' }" />
      </div>

      <p class="text-xs text-slate-400 mb-4">{{ p.message }}</p>

      <!-- Control Buttons -->
      <div class="flex gap-3" v-if="!p.finished">
        <button v-if="!paused()" @click="doControl('pause')"
          class="px-4 py-2 text-xs rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-400 hover:bg-amber-500/20">
          暂停
        </button>
        <button v-else @click="doControl('resume')"
          class="px-4 py-2 text-xs rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 hover:bg-emerald-500/20">
          恢复
        </button>
        <button @click="doControl('abort')"
          class="px-4 py-2 text-xs rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 hover:bg-rose-500/20">
          中止
        </button>
      </div>
    </div>

    <div v-else class="bg-slate-900/20 border border-dashed border-slate-800/50 rounded-xl p-8 text-center">
      <!-- Idle state -->
      <p class="text-sm text-slate-500">当前无运行中任务，点击上方卡片触发</p>
    </div>

    <!-- Recent Tasks -->
    <div>
      <h3 class="text-sm font-semibold text-slate-300 mb-3">最近任务记录</h3>
      <div class="bg-slate-900/40 border border-slate-800/50 rounded-xl overflow-hidden">
        <table class="w-full text-sm">
          <thead><tr class="text-slate-500 text-xs border-b border-slate-800/50"><th class="p-3 text-left">类型</th><th class="p-3 text-left">状态</th><th class="p-3 text-left">进度</th><th class="p-3 text-left">时间</th></tr></thead>
          <tbody>
            <tr v-for="t in taskList.slice(0, 20)" :key="t.id" class="border-b border-slate-800/20 text-slate-400">
              <td class="p-3">{{ taskTypeMap[t.task_type] || t.task_type }}</td>
              <td class="p-3"><span :class="t.status === 'success' ? 'text-emerald-400' : t.status === 'failed' ? 'text-rose-400' : 'text-slate-500'">{{ t.status }}</span></td>
              <td class="p-3 font-mono text-xs">{{ t.done_count }}/{{ t.done_count + t.skipped_count + t.failed_count }}</td>
              <td class="p-3 text-xs">{{ t.started_at?.slice(0, 16) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
