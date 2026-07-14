<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { RouterView } from 'vue-router'
import { useDashboardStore } from './stores/dashboard'
import { useTasksStore } from './stores/tasks'
import { useSettingsStore } from './stores/settings'
import { useToast } from './composables/useToast'
import { useClock } from './composables/useClock'
import Sidebar from './components/layout/Sidebar.vue'
import StatusDot from './components/ui/StatusDot.vue'
import ToastContainer from './components/ui/ToastContainer.vue'

const dashboard = useDashboardStore()
const tasks = useTasksStore()
const settings = useSettingsStore()
const { toasts, show } = useToast()
const clock = useClock()

onMounted(() => {
  dashboard.fetch()
  dashboard.fetchParquet()
  tasks.fetchHistory()
  settings.fetch()
  tasks.connectSSE()
  // Expose toast globally for child components
  ;(window as any).__toast = show
})

onUnmounted(() => { tasks.disconnectSSE() })

const menuItems = [
  { id: 'dashboard', name: '大盘概览', icon: 'M4 6a2 2 0 012-2h2a2 2 0 012 2v4a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v4a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z', path: '/' },
  { id: 'download', name: '盘后下载', icon: 'M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4', path: '/download' },
  { id: 'settings', name: '全局设置', icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4', path: '/settings' },
]
</script>

<template>
  <div id="app" class="flex h-full w-full bg-slate-950">
    <Sidebar :items="menuItems" />
    <div class="flex-1 flex flex-col overflow-hidden">
      <header class="h-16 flex items-center px-6 border-b border-slate-900 gap-3 shrink-0">
        <div class="w-8 h-8 rounded-lg bg-gradient-to-tr from-indigo-600 to-violet-500 flex items-center justify-center shadow-lg shadow-indigo-500/20">
          <svg class="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6" /></svg>
        </div>
        <div class="flex-1">
          <h1 class="text-lg font-semibold tracking-tight text-slate-50">通达信数据维护系统</h1>
          <p class="text-xs text-slate-500 font-mono">{{ clock }}</p>
        </div>
        <StatusDot :connected="dashboard.connected" :text="dashboard.connectionText" />
      </header>
      <main class="flex-1 overflow-y-auto px-8 pt-6 pb-8">
        <RouterView />
      </main>
    </div>
    <ToastContainer :toasts="toasts" />
  </div>
</template>
