<script setup lang="ts">

defineProps<{
  items: { id: string; name: string; icon: string; path: string }[]
  alertCount: number
}>()

</script>

<template>
  <aside class="w-64 shrink-0 bg-slate-900/50 border-r border-slate-800 flex flex-col" aria-label="主导航">
    <nav class="flex-1 p-4 space-y-1.5" role="navigation">
      <router-link v-for="item in items" :key="item.id" :to="item.path" custom v-slot="{ href, navigate, isActive }">
        <a :href="href" @click="navigate"
          :class="[
            'w-full flex items-center gap-3 px-4 py-3 rounded-xl text-sm font-medium transition-all duration-200 text-left block',
            isActive
              ? 'bg-indigo-600/10 text-indigo-400 border border-indigo-500/20'
              : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50 border border-transparent'
          ]">
          <svg class="w-5 h-5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" aria-hidden="true">
            <path stroke-linecap="round" stroke-linejoin="round" :d="item.icon" />
          </svg>
          {{ item.name }}
          <span v-if="item.id === 'alerts' && alertCount > 0"
            class="ml-auto px-2 py-0.5 text-xs rounded-full bg-rose-500/20 text-rose-400 border border-rose-500/30" aria-label="{{ alertCount }} 条未读告警">
            {{ alertCount }}
          </span>
        </a>
      </router-link>
    </nav>
    <div class="p-4 border-t border-slate-800/50">
      <p class="text-xs text-slate-600 text-center">通达信数据维护 v0.1.0</p>
    </div>
  </aside>
</template>
