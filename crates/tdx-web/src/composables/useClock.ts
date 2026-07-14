import { ref, onUnmounted } from 'vue'

export function useClock() {
  const time = ref('')
  const timer = setInterval(() => {
    time.value = new Date().toLocaleString('zh-CN', { hour12: false })
  }, 1000)

  onUnmounted(() => clearInterval(timer))
  return time
}
