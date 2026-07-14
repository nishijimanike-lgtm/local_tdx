<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, provide } from 'vue'
import type { DashboardStats, ParquetStats, TaskProgress, TaskLog, Alert, CalendarDay, ServerSettings, ConsoleLogEntry, Toast } from './types'
import { api } from './api/client'
import Sidebar from './components/Sidebar.vue'
import StatusIndicator from './components/StatusIndicator.vue'
import DashboardTab from './components/DashboardTab.vue'
import TasksTab from './components/TasksTab.vue'
import AfterMarketDownload from './components/AfterMarketDownload.vue'
import CalendarTab from './components/CalendarTab.vue'
import AlertsTab from './components/AlertsTab.vue'
import SettingsTab from './components/SettingsTab.vue'
import ToastContainer from './components/ToastContainer.vue'

const activeTab = ref('dashboard')

const menuItems = [
  { id: 'dashboard', name: '大盘概览', icon: 'M4 6a2 2 0 012-2h2a2 2 0 012 2v4a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v4a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z' },
  { id: 'download', name: '盘后下载', icon: 'M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4' },
  { id: 'tasks', name: '数据任务调度', icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z' },
  { id: 'calendar', name: '交易日历', icon: 'M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z' },
  { id: 'alerts', name: '告警看板', icon: 'M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z' },
  { id: 'settings', name: '全局设置', icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4' },
]

const currentTabName = computed(() => menuItems.find(i => i.id === activeTab.value)?.name ?? '数据平台')

// Clock
const formattedTime = ref('')
let clockTimer: number
const updateClock = () => { formattedTime.value = new Date().toLocaleString('zh-CN', { hour12: false }) }
updateClock()
clockTimer = setInterval(updateClock, 1000)

// Server status
const backendConnected = ref(false)
const connectionText = ref('未连接')

// Shared state
const dashboard = ref<DashboardStats>({
  adj_factor_tier: 'L3', last_probe_at: '', last_daily_update: '', last_adj_factor_update: '', calendar_source: 'index_derived',
  counts: { open_days: 0, xdxr_events: 0, adj_factor_symbols: 0 },
  daily_bar_range: { start: null, end: null },
})
const parquetStats = ref<ParquetStats>({ exists: false, parquet_dir: '', markets: {}, total_files: 0, total_size_mb: '0.00' })
const settings = ref<ServerSettings>({
  server: { host: '127.0.0.1', port: 8080 },
  paths: { tdx_data_dir: '', metadata_db_path: '', backup_dir: '', parquet_dir: '' },
  calendar: { benchmark_index_market: 1, benchmark_index_symbol: '000001', exchange: 'SSE' },
  tushare: { enabled: false, token: '', base_url: 'http://api.tushare.pro' },
  rate_limit: { market_hours_rps: 100, pre_post_market_rps: 150, off_hours_rps: 200 },
  adj_factor: { conflict_threshold_pct: 1.0, default_tier: 'L3' },
  alerts: { daily_completeness_threshold_pct: 95.0 },
  schedule: { daily_increment_cron: '', xdxr_sync_cron: '', adj_factor_sync_cron: '', calendar_check_cron: '', weekly_scan_cron: '' },
  retry: { max_attempts: 3, backoff_ms: 1000 },
})
const activeTaskProgress = ref<TaskProgress>({ task_id: 0, task_type: '', done: 0, skipped: 0, failed: 0, total: 0, message: '', finished: true, paused: false, aborted: false })
const alertsList = ref<Alert[]>([])
const calendarList = ref<CalendarDay[]>([])
const consoleLogs = ref<ConsoleLogEntry[]>([])
const toasts = ref<Toast[]>([])
const taskList = ref<TaskLog[]>([])

const isAnyTaskRunning = computed(() => !activeTaskProgress.value.finished)

// Toast
function showToast(message: string, type: 'success' | 'error' | 'info' = 'success') {
  const id = Date.now()
  toasts.value.push({ id, message, type })
  setTimeout(() => { toasts.value = toasts.value.filter(t => t.id !== id) }, 3500)
}

// Console
function appendConsoleLog(tag: string, text: string, tagClass = 'bg-indigo-600/10 text-indigo-400 border border-indigo-500/20', textClass = 'text-slate-200') {
  const time = new Date().toLocaleTimeString('zh-CN', { hour12: false })
  consoleLogs.value.push({ time, tag, text, tagClass, textClass })
  if (consoleLogs.value.length > 500) consoleLogs.value.shift()
}

// Dashboard
async function fetchDashboard() {
  try {
    const res = await api.get<DashboardStats>('/api/dashboard')
    dashboard.value = res
    backendConnected.value = true
    connectionText.value = `运行中 (端口: ${settings.value.server.port})`
  } catch { backendConnected.value = false; connectionText.value = '连接异常' }
}

async function fetchParquetStats() {
  try { parquetStats.value = await api.get<ParquetStats>('/api/parquet/stats') } catch { /* silently fail */ }
}

// Alerts
async function fetchAlerts() { try { alertsList.value = await api.get<Alert[]>('/api/alerts') } catch { /* */ } }
async function acknowledgeAlert(id: number) {
  try { await api.patch(`/api/alerts/${id}/acknowledge`); await fetchAlerts(); showToast('告警已确认') } catch { showToast('确认失败', 'error') }
}

// Tasks
async function fetchTasks() { try { taskList.value = await api.get<TaskLog[]>('/api/tasks') } catch { /* */ } }
async function clearHistory() {
  if (!confirm('确定清除所有任务历史记录吗？此操作不可撤销（运行中的任务记录会保留）。')) return
  try { await api.delete('/api/tasks'); await fetchTasks(); showToast('历史记录已清除') }
  catch (e: any) { showToast(`清除失败: ${e.message}`, 'error') }
}
async function triggerTask(action: string) {
  try { appendConsoleLog('TASK', `触发任务: ${action}`); await api.post(`/api/tasks/${action}`); showToast('任务已触发'); fetchTasks() }
  catch (e: any) { showToast(`触发失败: ${e.message}`, 'error') }
}
async function controlTask(action: string) {
  try { appendConsoleLog('CONTROL', `发出指令: ${action}`); await api.post(`/api/tasks/control/${action}`); showToast(`指令 ${action} 执行成功`) }
  catch (e: any) { showToast(`指令失败: ${e.message}`, 'error') }
}

// Calendar
async function fetchCalendar(start: string, end: string) {
  try { calendarList.value = await api.get<CalendarDay[]>(`/api/calendar?start=${start}&end=${end}`) } catch { /* */ }
}

// Settings
async function fetchSettings() { try { settings.value = await api.get<ServerSettings>('/api/settings') } catch { /* */ } }
async function saveSettings() {
  try { await api.put('/api/settings', settings.value); showToast('设置已保存，部分更改需重启生效') } catch (e: any) { showToast(`保存失败: ${e.message}`, 'error') }
}

// SSE progress
let eventSource: EventSource | null = null
function subscribeProgress() {
  if (eventSource) eventSource.close()
  eventSource = new EventSource('/api/tasks/progress')
  eventSource.onmessage = (e) => {
    try {
      const data = JSON.parse(e.data) as TaskProgress
      activeTaskProgress.value = data
      appendConsoleLog(data.task_type, data.message,
        data.finished ? 'bg-emerald-600/10 text-emerald-400 border border-emerald-500/20' : 'bg-indigo-600/10 text-indigo-400 border border-indigo-500/20')
      if (data.finished && data.task_id > 0) {
        // Task just finished — refresh history so the completed row appears.
        // Small delay lets the backend's finish() DB write land first.
        setTimeout(() => { fetchTasks() }, 300)
      }
    } catch { /* */ }
  }
}

// Init
onMounted(() => {
  fetchDashboard()
  fetchParquetStats()
  fetchAlerts()
  fetchTasks()
  fetchSettings()
  subscribeProgress()
})

onUnmounted(() => { clearInterval(clockTimer); eventSource?.close() })

// Provide for child components
provide('dashboard', dashboard)
provide('parquetStats', parquetStats)
provide('settings', settings)
provide('activeTaskProgress', activeTaskProgress)
provide('alertsList', alertsList)
provide('calendarList', calendarList)
provide('consoleLogs', consoleLogs)
provide('toasts', toasts)
provide('taskList', taskList)
provide('isAnyTaskRunning', isAnyTaskRunning)
provide('backendConnected', backendConnected)
provide('fetchDashboard', fetchDashboard)
provide('fetchParquetStats', fetchParquetStats)
provide('fetchAlerts', fetchAlerts)
provide('acknowledgeAlert', acknowledgeAlert)
provide('fetchTasks', fetchTasks)
provide('clearHistory', clearHistory)
provide('triggerTask', triggerTask)
provide('controlTask', controlTask)
provide('fetchCalendar', fetchCalendar)
provide('fetchSettings', fetchSettings)
provide('saveSettings', saveSettings)
provide('showToast', showToast)
provide('appendConsoleLog', appendConsoleLog)
</script>

<template>
  <div id="app" class="flex h-full w-full bg-slate-950" v-cloak>
    <Sidebar :items="menuItems" :active-tab="activeTab" @select="activeTab = $event" :alert-count="alertsList.filter(a => !a.acknowledged).length" />

    <div class="flex-1 flex flex-col overflow-hidden">
      <!-- Header -->
      <header class="h-16 flex items-center px-6 border-b border-slate-900 gap-3 shrink-0">
        <div class="w-8 h-8 rounded-lg bg-gradient-to-tr from-indigo-600 to-violet-500 flex items-center justify-center shadow-lg shadow-indigo-500/20">
          <svg class="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6" /></svg>
        </div>
        <div>
          <h1 class="text-lg font-display font-semibold tracking-tight text-slate-50">{{ currentTabName }}</h1>
          <p class="text-xs text-slate-500 font-mono">{{ formattedTime }}</p>
        </div>
        <div class="ml-auto">
          <StatusIndicator :connected="backendConnected" :text="connectionText" />
        </div>
      </header>

      <!-- Content -->
      <main class="flex-1 overflow-y-auto px-8 pt-6 pb-8 relative z-10">
        <DashboardTab v-if="activeTab === 'dashboard'" />
        <AfterMarketDownload v-if="activeTab === 'download'" />
        <TasksTab v-if="activeTab === 'tasks'" />
        <CalendarTab v-if="activeTab === 'calendar'" />
        <AlertsTab v-if="activeTab === 'alerts'" />
        <SettingsTab v-if="activeTab === 'settings'" />
      </main>
    </div>

    <ToastContainer :toasts="toasts" />
  </div>
</template>
