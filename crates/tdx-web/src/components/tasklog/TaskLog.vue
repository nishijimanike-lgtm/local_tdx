<script setup lang="ts">
import { onMounted } from 'vue'
import { useTasksStore } from '../../stores/tasks'

const tasks = useTasksStore()

const labelMap: Record<string, string> = {
  calendar_sync: '交易日历', daily_bar_full: '日线全量', daily_bar_update: '日线增量',
  daily_bar_gap_fill: '空缺填补', xdxr_sync: 'XDXR 同步', adj_factor_update: '复权因子',
  daily_bar_scan: '完整性扫描', local_import: '本地导入',
}

function label(t: string) { return labelMap[t] || t }

onMounted(() => tasks.fetchHistory())
</script>

<template>
  <div class="space-y-4">
    <div class="glass-panel rounded-xl border border-slate-800/50 overflow-hidden">
      <div class="px-6 py-4 border-b border-slate-800/50 flex items-center justify-between">
        <div class="flex items-center gap-2.5">
          <div class="p-1.5 rounded-lg bg-cyan-500/10 border border-cyan-500/20 text-cyan-400">
            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" /></svg>
          </div>
          <h2 class="text-sm font-semibold text-slate-200">任务完成情况</h2>
        </div>
        <span class="text-xs text-slate-500 font-mono">{{ tasks.history.length }} 条记录</span>
      </div>

      <div v-if="tasks.history.length === 0" class="px-6 py-16 text-center text-slate-500 text-sm">
        暂无任务记录
      </div>

      <table v-else class="w-full text-sm">
        <thead>
          <tr class="text-slate-500 text-xs border-b border-slate-800/50 bg-slate-900/30">
            <th class="px-6 py-3 text-left font-medium">类型</th>
            <th class="px-6 py-3 text-left font-medium">状态</th>
            <th class="px-6 py-3 text-right font-medium">完成</th>
            <th class="px-6 py-3 text-right font-medium">跳过</th>
            <th class="px-6 py-3 text-right font-medium">失败</th>
            <th class="px-6 py-3 text-left font-medium">时间</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="t in tasks.history" :key="t.id"
            class="border-b border-slate-800/20 hover:bg-slate-800/20 transition-colors">
            <td class="px-6 py-3 font-medium text-slate-300">{{ label(t.task_type) }}</td>
            <td class="px-6 py-3">
              <span class="text-xs px-2 py-0.5 rounded-full font-mono"
                :class="t.status === 'success' ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20' :
                        t.status === 'partial' ? 'bg-amber-500/10 text-amber-400 border border-amber-500/20' :
                        t.status === 'failed' ? 'bg-rose-500/10 text-rose-400 border border-rose-500/20' :
                        'bg-slate-500/10 text-slate-400 border border-slate-500/20'">
                {{ t.status }}
              </span>
            </td>
            <td class="px-6 py-3 text-right font-mono text-xs text-emerald-400">{{ t.done_count }}</td>
            <td class="px-6 py-3 text-right font-mono text-xs text-slate-400">{{ t.skipped_count }}</td>
            <td class="px-6 py-3 text-right font-mono text-xs" :class="t.failed_count > 0 ? 'text-rose-400' : 'text-slate-400'">{{ t.failed_count }}</td>
            <td class="px-6 py-3 text-xs text-slate-500 font-mono whitespace-nowrap">{{ t.started_at?.slice(0, 16) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
