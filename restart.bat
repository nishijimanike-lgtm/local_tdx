@echo off
chcp 65001 >nul
cd /d %~dp0

echo ========================================
echo   通达信数据维护系统 - 构建并重启
echo ========================================

echo.
echo [1/3] 终止已有进程...
taskkill /F /IM tdx-maintain-server.exe >nul 2>&1
if %errorlevel%==0 (
    echo   已终止旧进程
) else (
    echo   没有运行中的进程
)
timeout /t 1 /nobreak >nul

echo.
echo [2/3] 构建项目...

if not exist "crates\tdx-web\dist\index.html" (
    echo   检测到前端尚未构建，正在编译前端...
    pushd crates\tdx-web
    call npm install && call npm run build
    popd
)

set PATH=C:\Users\zhang\.cargo\bin;C:\rtools45\x86_64-w64-mingw32.static.posix\bin;%PATH%
if "%1"=="release" (
    echo   模式: Release
    cargo build --release
    set BIN=target\release\tdx-maintain-server.exe
) else (
    echo   模式: Debug
    cargo build
    set BIN=target\debug\tdx-maintain-server.exe
)

if not exist "%BIN%" (
    echo   构建失败: 找不到 %BIN%
    pause
    exit /b 1
)
echo   构建完成

echo.
echo [3/3] 启动服务...
start "" /B "%BIN%"
timeout /t 2 /nobreak >nul
echo   服务已启动: http://127.0.0.1:8080

echo.
echo ========================================
echo   完成
echo ========================================
