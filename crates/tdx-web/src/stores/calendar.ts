import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api } from '../api/client'
import type { CalendarDay } from '../types'

export const useCalendarStore = defineStore('calendar', () => {
  const list = ref<CalendarDay[]>([])

  async function fetch(start: string, end: string) {
    try { list.value = await api.get<CalendarDay[]>(`/api/calendar?start=${start}&end=${end}`) } catch { /* */ }
  }

  return { list, fetch }
})
