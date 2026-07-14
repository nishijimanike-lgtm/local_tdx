<script setup lang="ts">
import { useSettingsStore } from '../../stores/settings'
const settings = useSettingsStore()

function toast(msg: string, type = 'success') { (window as any).__toast?.(msg, type) }

async function save() {
  try { await settings.save(); toast('设置已保存，部分更改需重启生效') }
  catch (e: any) { toast(`保存失败: ${e.message}`, 'error') }
}
</script>

<template>
  <div class="max-w-3xl space-y-5">
    <!-- Server -->
    <div class="glass-panel rounded-xl border border-slate-800/50 p-6">
      <div class="flex items-center gap-2.5 mb-5">
        <div class="p-1.5 rounded-lg bg-indigo-500/10 border border-indigo-500/20 text-indigo-400">
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" /></svg>
        </div>
        <h3 class="text-sm font-semibold text-slate-200">服务器</h3>
      </div>
      <div class="grid grid-cols-2 gap-4">
        <div><label class="text-xs text-slate-500 block mb-1.5">Host</label><input v-model="settings.data.server.host" class="w-full bg-slate-800/40 border border-slate-700/50 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500/40 transition-colors" /></div>
        <div><label class="text-xs text-slate-500 block mb-1.5">Port</label><input v-model.number="settings.data.server.port" type="number" class="w-full bg-slate-800/40 border border-slate-700/50 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500/40 transition-colors" /></div>
      </div>
    </div>

    <!-- Paths -->
    <div class="glass-panel rounded-xl border border-slate-800/50 p-6">
      <div class="flex items-center gap-2.5 mb-5">
        <div class="p-1.5 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-400">
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" /></svg>
        </div>
        <h3 class="text-sm font-semibold text-slate-200">本地路径</h3>
      </div>
      <div class="space-y-4">
        <div v-for="p in [
          { key: 'tdx_data_dir', label: 'TDX 数据目录' },
          { key: 'metadata_db_path', label: '元数据库路径' },
          { key: 'backup_dir', label: '备份目录' },
          { key: 'parquet_dir', label: 'Parquet 目录' },
        ]" :key="p.key">
          <label class="text-xs text-slate-500 block mb-1.5">{{ p.label }}</label>
          <input :value="(settings.data.paths as any)[p.key]" @input="(e: any) => (settings.data.paths as any)[p.key] = e.target.value"
            class="w-full bg-slate-800/40 border border-slate-700/50 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono focus:outline-none focus:border-amber-500/40 transition-colors" />
        </div>
      </div>
    </div>

    <!-- Save -->
    <div class="flex items-center gap-3 pt-2">
      <button @click="save"
        class="px-6 py-2.5 bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg text-sm font-medium transition-colors shadow-lg shadow-indigo-500/20">
        保存设置
      </button>
      <span class="text-xs text-slate-600">部分更改需重启服务生效</span>
    </div>
  </div>
</template>
