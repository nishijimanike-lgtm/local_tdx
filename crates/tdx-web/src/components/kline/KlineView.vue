<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import * as echarts from 'echarts'

interface Stock {
  market: string
  symbol: string
  name: string
}

interface KlineBar {
  date: string
  open: number
  high: number
  low: number
  close: number
  volume: number
  amount: number
}

// State
const searchQuery = ref('')
const searchResults = ref<Stock[]>([])
const selectedStock = ref<Stock>({ market: 'sh', symbol: '000001', name: '上证指数' })
const adjustMode = ref<'none' | 'forward' | 'backward'>('none')
const loading = ref(false)
const errorMsg = ref('')
const showDropdown = ref(false)

// ECharts
const chartRef = ref<HTMLDivElement | null>(null)
let chartInstance: echarts.ECharts | null = null

// Fetch Search Results
let searchTimeout: number | null = null
const onSearchInput = () => {
  if (searchTimeout) window.clearTimeout(searchTimeout)
  if (!searchQuery.value.trim()) {
    searchResults.value = []
    showDropdown.value = false
    return
  }
  searchTimeout = window.setTimeout(async () => {
    try {
      const resp = await fetch(`/api/stocks/search?q=${encodeURIComponent(searchQuery.value)}`)
      if (resp.ok) {
        searchResults.value = await resp.json()
        showDropdown.value = searchResults.value.length > 0
      }
    } catch (e) {
      console.error('Failed to search stocks:', e)
    }
  }, 300)
}

// Select Stock
const selectStock = (stock: Stock) => {
  selectedStock.value = stock
  searchQuery.value = `${stock.name} (${stock.symbol})`
  showDropdown.value = false
}

// Clear Search
const clearSearch = () => {
  searchQuery.value = ''
  searchResults.value = []
  showDropdown.value = false
}

// Hide dropdown on click outside
const onClickOutside = (e: MouseEvent) => {
  const target = e.target as HTMLElement
  if (!target.closest('.search-container')) {
    showDropdown.value = false
  }
}

// Fetch K-line data
const fetchKlineData = async () => {
  const stock = selectedStock.value
  if (!stock) return

  loading.value = true
  errorMsg.value = ''

  try {
    const resp = await fetch(`/api/stock/kline?market=${stock.market}&symbol=${stock.symbol}&adjust=${adjustMode.value}`)
    if (!resp.ok) {
      const errText = await resp.text()
      throw new Error(errText || '获取 K 线数据失败')
    }
    const data: KlineBar[] = await resp.json()
    if (data.length === 0) {
      throw new Error('未获取到该标的的 K 线数据 (文件可能不存在或为空)')
    }
    renderChart(data)
  } catch (e: any) {
    console.error(e)
    errorMsg.value = e.message || '网络请求错误，请稍后重试'
    if (chartInstance) {
      chartInstance.clear()
    }
  } finally {
    loading.value = false
  }
}

// Compute Moving Average
const calculateMA = (dayCount: number, data: KlineBar[]) => {
  const result: (number | string)[] = []
  for (let i = 0; i < data.length; i++) {
    if (i < dayCount - 1) {
      result.push('-')
      continue
    }
    let sum = 0
    for (let j = 0; j < dayCount; j++) {
      sum += data[i - j].close
    }
    result.push(Number((sum / dayCount).toFixed(2)))
  }
  return result
}

// Render Chart
const renderChart = (data: KlineBar[]) => {
  if (!chartRef.value) return

  if (!chartInstance) {
    chartInstance = echarts.init(chartRef.value, 'dark')
  }

  // Map data
  const dates = data.map(bar => bar.date)
  // ECharts Candlestick values format: [open, close, lowest, highest]
  const ohlc = data.map(bar => [bar.open, bar.close, bar.low, bar.high])
  const volumes = data.map((bar) => {
    return {
      value: bar.volume,
      itemStyle: {
        color: bar.close >= bar.open ? '#f43f5e' : '#10b981' // Red for up, Green for down
      }
    }
  })

  const ma5 = calculateMA(5, data)
  const ma10 = calculateMA(10, data)
  const ma20 = calculateMA(20, data)
  const ma30 = calculateMA(30, data)

  const option: echarts.EChartsOption = {
    backgroundColor: 'transparent',
    title: {
      text: `${selectedStock.value.name} (${selectedStock.value.symbol})`,
      left: 10,
      top: 10,
      textStyle: {
        color: '#f8fafc',
        fontSize: 16,
        fontWeight: 'bold'
      }
    },
    tooltip: {
      trigger: 'axis',
      axisPointer: {
        type: 'cross',
        label: {
          backgroundColor: '#334155'
        }
      },
      borderWidth: 1,
      borderColor: '#334155',
      backgroundColor: '#0f172a',
      textStyle: {
        color: '#e2e8f0'
      },
      position: function (pos, _params, _el, _elRect, size) {
        const obj: Record<string, number> = { top: 10 }
        obj[['left', 'right'][+(pos[0] < size.viewSize[0] / 2)]] = 30
        return obj
      },
      formatter: function (params: any) {
        let res = ''
        let param = params.find((p: any) => p.seriesName === 'K线')
        if (param) {
          const date = param.name
          const open = param.data[1]
          const close = param.data[2]
          const low = param.data[3]
          const high = param.data[4]
          const diff = close - open
          const changePct = ((diff / open) * 100).toFixed(2)
          const changeCls = diff >= 0 ? 'text-rose-500' : 'text-emerald-500'

          let volParam = params.find((p: any) => p.seriesName === '成交量')
          const volume = volParam ? volParam.data.value || volParam.data : 0
          const formattedVol = (volume / 10000).toFixed(2) + ' 万股'

          res += `<div class="font-mono text-xs space-y-1">`
          res += `<div class="text-slate-400 font-bold mb-1 border-b border-slate-800 pb-1">${date}</div>`
          res += `<div>开盘: <span class="text-slate-100">${open.toFixed(2)}</span></div>`
          res += `<div>收盘: <span class="text-slate-100">${close.toFixed(2)}</span></div>`
          res += `<div>最高: <span class="text-slate-100">${high.toFixed(2)}</span></div>`
          res += `<div>最低: <span class="text-slate-100">${low.toFixed(2)}</span></div>`
          res += `<div>涨跌: <span class="${changeCls}">${diff >= 0 ? '+' : ''}${diff.toFixed(2)} (${diff >= 0 ? '+' : ''}${changePct}%)</span></div>`
          res += `<div>成交: <span class="text-slate-100">${formattedVol}</span></div>`
          res += `</div>`
        }
        return res
      }
    },
    axisPointer: {
      link: [{ xAxisIndex: 'all' }]
    },
    grid: [
      {
        left: '5%',
        right: '4%',
        top: '12%',
        height: '63%'
      },
      {
        left: '5%',
        right: '4%',
        top: '80%',
        height: '14%'
      }
    ],
    xAxis: [
      {
        type: 'category',
        data: dates,
        boundaryGap: false,
        axisLine: { onZero: false, lineStyle: { color: '#334155' } },
        splitLine: { show: false },
        min: 'dataMin',
        max: 'dataMax',
        axisPointer: {
          z: 100
        }
      },
      {
        type: 'category',
        gridIndex: 1,
        data: dates,
        boundaryGap: false,
        axisLine: { onZero: false, lineStyle: { color: '#334155' } },
        axisLabel: { show: false },
        splitLine: { show: false },
        min: 'dataMin',
        max: 'dataMax'
      }
    ],
    yAxis: [
      {
        scale: true,
        splitArea: { show: false },
        axisLine: { lineStyle: { color: '#334155' } },
        splitLine: { lineStyle: { color: '#1e293b' } }
      },
      {
        scale: true,
        gridIndex: 1,
        splitNumber: 2,
        axisLabel: { show: false },
        axisLine: { show: false },
        axisTick: { show: false },
        splitLine: { show: false }
      }
    ],
    dataZoom: [
      {
        type: 'inside',
        xAxisIndex: [0, 1],
        start: Math.max(0, 100 - (120 / data.length) * 100), // Default to last 120 trading days
        end: 100
      },
      {
        show: true,
        xAxisIndex: [0, 1],
        type: 'slider',
        top: '95%',
        start: Math.max(0, 100 - (120 / data.length) * 100),
        end: 100,
        backgroundColor: '#0f172a',
        borderColor: '#1e293b',
        fillerColor: 'rgba(99, 102, 241, 0.1)',
        handleStyle: {
          color: '#6366f1'
        }
      }
    ],
    series: [
      {
        name: 'K线',
        type: 'candlestick',
        data: ohlc,
        itemStyle: {
          color: '#f43f5e',
          color0: '#10b981',
          borderColor: '#f43f5e',
          borderColor0: '#10b981'
        }
      },
      {
        name: 'MA5',
        type: 'line',
        data: ma5,
        smooth: true,
        showSymbol: false,
        lineStyle: { opacity: 0.8, width: 1.2, color: '#f59e0b' }
      },
      {
        name: 'MA10',
        type: 'line',
        data: ma10,
        smooth: true,
        showSymbol: false,
        lineStyle: { opacity: 0.8, width: 1.2, color: '#06b6d4' }
      },
      {
        name: 'MA20',
        type: 'line',
        data: ma20,
        smooth: true,
        showSymbol: false,
        lineStyle: { opacity: 0.8, width: 1.2, color: '#a855f7' }
      },
      {
        name: 'MA30',
        type: 'line',
        data: ma30,
        smooth: true,
        showSymbol: false,
        lineStyle: { opacity: 0.8, width: 1.2, color: '#ec4899' }
      },
      {
        name: '成交量',
        type: 'bar',
        xAxisIndex: 1,
        yAxisIndex: 1,
        data: volumes
      }
    ]
  }

  chartInstance.setOption(option)
}

// Watchers
watch([selectedStock, adjustMode], () => {
  fetchKlineData()
})

const handleResize = () => {
  if (chartInstance) chartInstance.resize()
}

onMounted(() => {
  searchQuery.value = `${selectedStock.value.name} (${selectedStock.value.symbol})`
  fetchKlineData()
  window.addEventListener('click', onClickOutside)
  window.addEventListener('resize', handleResize)
})

onUnmounted(() => {
  if (searchTimeout) window.clearTimeout(searchTimeout)
  window.removeEventListener('click', onClickOutside)
  window.removeEventListener('resize', handleResize)
  if (chartInstance) {
    chartInstance.dispose()
  }
})
</script>

<template>
  <div class="space-y-6 flex flex-col h-full overflow-hidden">
    <!-- Options Bar -->
    <div class="glass-panel rounded-xl p-4 border border-slate-800/50 flex flex-wrap items-center gap-4 shrink-0 justify-between relative z-50">
      <!-- Search Input -->
      <div class="search-container relative w-72">
        <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none text-slate-500">
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svg>
        </div>
        <input
          v-model="searchQuery"
          @input="onSearchInput"
          @focus="showDropdown = searchResults.length > 0"
          type="text"
          placeholder="输入股票代码、名称或拼音首字母..."
          class="w-full bg-slate-900 border border-slate-800 rounded-lg pl-9 pr-9 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500/50 transition-colors placeholder:text-slate-500 font-sans"
        />
        <button
          v-if="searchQuery"
          @click="clearSearch"
          type="button"
          class="absolute inset-y-0 right-0 pr-3 flex items-center text-slate-500 hover:text-slate-300 transition-colors"
        >
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>

        <!-- Dropdown menu -->
        <div v-if="showDropdown" class="absolute z-50 left-0 right-0 mt-2 bg-slate-900 border border-slate-800 rounded-lg max-h-60 overflow-y-auto shadow-2xl">
          <div
            v-for="item in searchResults"
            :key="item.market + item.symbol"
            @click="selectStock(item)"
            class="flex items-center justify-between px-4 py-2 hover:bg-slate-800 cursor-pointer transition-colors border-b border-slate-800/50 last:border-0"
          >
            <div class="flex items-center gap-2">
              <span class="text-xs uppercase bg-slate-800 text-slate-400 px-1.5 py-0.5 rounded font-mono font-bold">{{ item.market }}</span>
              <span class="text-sm text-slate-200">{{ item.name }}</span>
            </div>
            <span class="text-xs text-slate-500 font-mono font-bold">{{ item.symbol }}</span>
          </div>
        </div>
      </div>

      <!-- Price Adjustment Buttons -->
      <div class="flex items-center bg-slate-900 border border-slate-800 rounded-lg p-1">
        <button
          @click="adjustMode = 'none'"
          class="px-3 py-1.5 rounded-md text-xs font-medium transition-colors"
          :class="adjustMode === 'none' ? 'bg-indigo-600 text-white shadow' : 'text-slate-400 hover:text-slate-200'"
        >
          不复权
        </button>
        <button
          @click="adjustMode = 'forward'"
          class="px-3 py-1.5 rounded-md text-xs font-medium transition-colors"
          :class="adjustMode === 'forward' ? 'bg-indigo-600 text-white shadow' : 'text-slate-400 hover:text-slate-200'"
        >
          前复权
        </button>
        <button
          @click="adjustMode = 'backward'"
          class="px-3 py-1.5 rounded-md text-xs font-medium transition-colors"
          :class="adjustMode === 'backward' ? 'bg-indigo-600 text-white shadow' : 'text-slate-400 hover:text-slate-200'"
        >
          后复权
        </button>
      </div>
    </div>

    <!-- Chart Panel -->
    <div class="flex-1 min-h-[450px] relative glass-panel rounded-xl border border-slate-800/50 p-6 overflow-hidden flex flex-col">
      <!-- Loading Overlay -->
      <div v-if="loading" class="absolute inset-0 bg-slate-950/60 backdrop-blur-sm z-40 flex items-center justify-center">
        <div class="flex flex-col items-center gap-3">
          <svg class="animate-spin h-8 w-8 text-indigo-500" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
          </svg>
          <span class="text-xs text-slate-400 font-medium">正在读取并计算价格数据...</span>
        </div>
      </div>

      <!-- Error Alert -->
      <div v-if="errorMsg" class="absolute inset-0 bg-slate-950/40 backdrop-blur-sm z-40 flex items-center justify-center p-6">
        <div class="max-w-md w-full bg-rose-500/10 border border-rose-500/25 rounded-xl p-5 text-center flex flex-col items-center gap-3">
          <div class="w-10 h-10 rounded-full bg-rose-500/20 text-rose-400 flex items-center justify-center">
            <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <h4 class="text-sm font-semibold text-slate-100">无法读取行情数据</h4>
          <p class="text-xs text-slate-400 font-sans leading-relaxed">{{ errorMsg }}</p>
          <button @click="fetchKlineData" class="mt-2 text-xs font-semibold px-4 py-2 bg-slate-900 border border-slate-800 text-slate-300 hover:text-slate-100 rounded-lg transition-colors">
            重试
          </button>
        </div>
      </div>

      <!-- ECharts Container -->
      <div ref="chartRef" class="w-full flex-1"></div>
    </div>
  </div>
</template>
