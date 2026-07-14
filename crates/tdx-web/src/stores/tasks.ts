import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '../api/client'
import type { TaskProgress, TaskLog } from '../types'

export const useTasksStore = defineStore('tasks', () => {
  const progress = ref<TaskProgress>({ task_id: 0, task_type: '', done: 0, skipped: 0, failed: 0, total: 0, message: '', finished: true, paused: false, aborted: false })
  const history = ref<TaskLog[]>([])
  const isRunning = computed(() => !progress.value.finished)

  let eventSource: EventSource | null = null

  function connectSSE() {
    if (eventSource) eventSource.close()
    eventSource = new EventSource('/api/tasks/progress')
    eventSource.onmessage = (e) => {
      try {
        progress.value = JSON.parse(e.data) as TaskProgress
        if (progress.value.finished && progress.value.task_id > 0) {
          setTimeout(() => fetchHistory(), 300)
        }
      } catch { /* */ }
    }
  }

  function disconnectSSE() { eventSource?.close(); eventSource = null }

  async function fetchHistory() { try { history.value = await api.get<TaskLog[]>('/api/tasks') } catch { /* */ } }

  async function trigger(action: string) {
    try { await api.post(`/api/tasks/${action}`); await fetchHistory() }
    catch (e: any) { throw e }
  }

  async function control(action: string) {
    try { await api.post(`/api/tasks/control/${action}`) } catch (e: any) { throw e }
  }

  async function clearHistory() {
    if (!confirm('确定清除所有任务历史记录？')) return
    try { await api.delete('/api/tasks'); await fetchHistory() } catch { /* */ }
  }

  return { progress, history, isRunning, connectSSE, disconnectSSE, fetchHistory, trigger, control, clearHistory }
})
