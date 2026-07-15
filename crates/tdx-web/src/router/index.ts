import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'dashboard', component: () => import('../components/dashboard/DashboardView.vue') },
    { path: '/download', name: 'download', component: () => import('../components/download/AfterMarketDownload.vue') },
    { path: '/settings', name: 'settings', component: () => import('../components/settings/SettingsView.vue') },
    { path: '/tasklog', name: 'tasklog', component: () => import('../components/tasklog/TaskLog.vue') },
    { path: '/kline', name: 'kline', component: () => import('../components/kline/KlineView.vue') },
    { path: '/qlib', name: 'qlib', component: () => import('../components/qlib/QlibDumpView.vue') },
    { path: '/checker', name: 'checker', component: () => import('../components/checker/DataCheckerView.vue') },
  ],
})

export default router
