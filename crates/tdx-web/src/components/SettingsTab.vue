<script setup lang="ts">
import { inject, type Ref } from 'vue'
import type { ServerSettings } from '../types'
const settings = inject<Ref<ServerSettings>>('settings')!
const saveSettings = inject<() => Promise<void>>('saveSettings')!
</script>
<template>
  <div class="space-y-6 max-w-3xl">
    <div class="bg-slate-900/30 border border-slate-800/50 rounded-xl p-6">
      <h3 class="text-sm font-semibold text-slate-300 mb-4">服务器</h3>
      <div class="grid grid-cols-2 gap-4">
        <div><label class="text-xs text-slate-500 block mb-1">Host</label><input v-model="settings.server.host" class="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200" /></div>
        <div><label class="text-xs text-slate-500 block mb-1">Port</label><input v-model.number="settings.server.port" type="number" class="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200" /></div>
      </div>
    </div>
    <div class="bg-slate-900/30 border border-slate-800/50 rounded-xl p-6">
      <h3 class="text-sm font-semibold text-slate-300 mb-4">本地路径</h3>
      <div class="space-y-4">
        <div><label class="text-xs text-slate-500 block mb-1">TDX 数据目录</label><input v-model="settings.paths.tdx_data_dir" class="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200" /></div>
        <div><label class="text-xs text-slate-500 block mb-1">元数据库路径</label><input v-model="settings.paths.metadata_db_path" class="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200" /></div>
        <div><label class="text-xs text-slate-500 block mb-1">备份目录</label><input v-model="settings.paths.backup_dir" class="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200" /></div>
        <div><label class="text-xs text-slate-500 block mb-1">Parquet 目录</label><input v-model="settings.paths.parquet_dir" class="w-full bg-slate-800/50 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200" /></div>
      </div>
    </div>
    <button @click="saveSettings" class="px-6 py-3 bg-indigo-600 hover:bg-indigo-500 text-white rounded-xl text-sm font-medium transition-colors">保存设置</button>
  </div>
</template>
