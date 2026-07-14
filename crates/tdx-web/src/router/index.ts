import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'dashboard', component: () => import('../components/dashboard/DashboardView.vue') },
    { path: '/download', name: 'download', component: () => import('../components/download/AfterMarketDownload.vue') },
    { path: '/tasks', name: 'tasks', component: () => import('../components/tasks/TasksView.vue') },
    { path: '/calendar', name: 'calendar', component: () => import('../components/calendar/CalendarView.vue') },
    { path: '/alerts', name: 'alerts', component: () => import('../components/alerts/AlertsView.vue') },
    { path: '/settings', name: 'settings', component: () => import('../components/settings/SettingsView.vue') },
  ],
})

export default router
