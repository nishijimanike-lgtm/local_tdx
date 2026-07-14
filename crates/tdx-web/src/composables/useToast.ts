import { ref, type Ref } from 'vue'

export interface Toast {
  id: number
  message: string
  type: 'success' | 'error' | 'info'
}

export function useToast() {
  const toasts: Ref<Toast[]> = ref([])

  function show(message: string, type: 'success' | 'error' | 'info' = 'success') {
    const id = Date.now()
    toasts.value.push({ id, message, type })
    setTimeout(() => { toasts.value = toasts.value.filter(t => t.id !== id) }, 3500)
  }

  return { toasts, show }
}
