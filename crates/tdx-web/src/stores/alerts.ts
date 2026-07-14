import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api } from '../api/client'
import type { Alert } from '../types'

export const useAlertsStore = defineStore('alerts', () => {
  const list = ref<Alert[]>([])
  const unreadCount = computed(() => list.value.filter(a => !a.acknowledged).length)

  async function fetch() { try { list.value = await api.get<Alert[]>('/api/alerts') } catch { /* */ } }

  async function acknowledge(id: number) {
    await api.patch(`/api/alerts/${id}/acknowledge`)
    await fetch()
  }

  return { list, unreadCount, fetch, acknowledge }
})
