# 通达信数据维护系统 - 构建并重启脚本
# Usage: .\restart.ps1          (debug 模式)
#        .\restart.ps1 -Release (release 模式)

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $projectRoot

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  通达信数据维护系统 - 构建并重启" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# 1. Kill existing process
Write-Host "`n[1/3] 终止已有进程..." -ForegroundColor Yellow
$existing = Get-Process -Name "tdx-maintain-server" -ErrorAction SilentlyContinue
if ($existing) {
    $existing | ForEach-Object {
        Write-Host "  -> 终止 PID: $($_.Id)" -ForegroundColor Gray
        Stop-Process -Id $_.Id -Force
    }
    Start-Sleep -Seconds 1
    Write-Host "  已终止所有旧进程" -ForegroundColor Green
} else {
    Write-Host "  没有运行中的进程" -ForegroundColor Green
}

# 也确保端口释放
$portCheck = netstat -ano | Select-String ":8080.*LISTENING"
if ($portCheck) {
    $pidMatch = [regex]::Match($portCheck, '\s+(\d+)\s*$')
    if ($pidMatch.Success) {
        $portPid = $pidMatch.Groups[1].Value
        Write-Host "  -> 释放端口 8080 (PID: $portPid)" -ForegroundColor Gray
        Stop-Process -Id $portPid -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 1
    }
}

# 2. Build
Write-Host "`n[2/3] 构建项目..." -ForegroundColor Yellow

$distHtml = "$projectRoot\crates\tdx-web\dist\index.html"
if (-not (Test-Path $distHtml)) {
    Write-Host "  检测到前端尚未构建，正在编译前端..." -ForegroundColor Yellow
    Push-Location "$projectRoot\crates\tdx-web"
    npm install
    npm run build
    Pop-Location
}

# Prepend cargo and rtools to PATH to ensure compiling succeeds in the local environment
$env:PATH = "C:\Users\zhang\.cargo\bin;C:\rtools45\x86_64-w64-mingw32.static.posix\bin;" + $env:PATH
if ($Release) {
    Write-Host "  模式: Release" -ForegroundColor Gray
    cargo build --release
    $binPath = "$projectRoot\target\release\tdx-maintain-server.exe"
} else {
    Write-Host "  模式: Debug" -ForegroundColor Gray
    cargo build
    $binPath = "$projectRoot\target\debug\tdx-maintain-server.exe"
}

if (-not (Test-Path $binPath)) {
    Write-Host "  构建失败: 找不到 $binPath" -ForegroundColor Red
    exit 1
}
Write-Host "  构建完成" -ForegroundColor Green

# 3. Start
Write-Host "`n[3/3] 启动服务..." -ForegroundColor Yellow
Start-Process -FilePath $binPath -WorkingDirectory $projectRoot -WindowStyle Hidden
Start-Sleep -Seconds 2

# Verify
$verify = netstat -ano | Select-String ":8080.*LISTENING"
if ($verify) {
    Write-Host "  服务启动成功: http://127.0.0.1:8080" -ForegroundColor Green
} else {
    Write-Host "  警告: 端口 8080 未检测到监听，请检查日志" -ForegroundColor Red
}

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "  完成" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
