# WebSocket 测试脚本
# 测试 ZeroClaw WebSocket 连接和消息处理

Write-Host "=== ZeroClaw WebSocket 诊断工具 ===" -ForegroundColor Cyan
Write-Host ""

# 检查服务是否运行
Write-Host "1. 检查服务状态..." -ForegroundColor Yellow
$coreProcess = Get-Process -Name "zeroclaw-core" -ErrorAction SilentlyContinue
$edgeProcess = Get-Process -Name "zeroclaw-edge" -ErrorAction SilentlyContinue

if ($coreProcess) {
    Write-Host "   ✓ zeroclaw-core 正在运行 (PID: $($coreProcess.Id))" -ForegroundColor Green
} else {
    Write-Host "   ✗ zeroclaw-core 未运行" -ForegroundColor Red
}

if ($edgeProcess) {
    Write-Host "   ✓ zeroclaw-edge 正在运行 (PID: $($edgeProcess.Id))" -ForegroundColor Green
} else {
    Write-Host "   ✗ zeroclaw-edge 未运行" -ForegroundColor Red
}

Write-Host ""

# 检查端口是否监听
Write-Host "2. 检查端口监听状态..." -ForegroundColor Yellow
$corePort = Get-NetTCPConnection -LocalPort 42618 -State Listen -ErrorAction SilentlyContinue
$edgePort = Get-NetTCPConnection -LocalPort 42617 -State Listen -ErrorAction SilentlyContinue

if ($corePort) {
    Write-Host "   ✓ Core 端口 42618 正在监听" -ForegroundColor Green
} else {
    Write-Host "   ✗ Core 端口 42618 未监听" -ForegroundColor Red
}

if ($edgePort) {
    Write-Host "   ✓ Edge 端口 42617 正在监听" -ForegroundColor Green
} else {
    Write-Host "   ✗ Edge 端口 42617 未监听" -ForegroundColor Red
}

Write-Host ""

# 测试 HTTP 连接
Write-Host "3. 测试 HTTP 连接..." -ForegroundColor Yellow
try {
    $response = Invoke-WebRequest -Uri "http://127.0.0.1:42617/health" -TimeoutSec 5 -ErrorAction Stop
    Write-Host "   ✓ Edge 健康检查成功: $($response.StatusCode)" -ForegroundColor Green
} catch {
    Write-Host "   ✗ Edge 健康检查失败: $($_.Exception.Message)" -ForegroundColor Red
}

Write-Host ""

# 如果服务未运行，提示启动
if (-not $coreProcess -or -not $edgeProcess) {
    Write-Host "请先启动服务：" -ForegroundColor Yellow
    Write-Host "  1. 启动 Core: powershell -ExecutionPolicy Bypass -File .\start-zeroclaw-core.ps1 --project-root D:\workspace\axi-research\zeroclaw"
    Write-Host "  2. 启动 Edge: powershell -ExecutionPolicy Bypass -File .\start-zeroclaw-edge.ps1 --project-root D:\workspace\axi-research\zeroclaw"
    Write-Host ""
    exit
}

Write-Host "4. 测试 WebSocket 连接..." -ForegroundColor Yellow
Write-Host "   提示: PowerShell 原生不支持 WebSocket 客户端" -ForegroundColor Gray
Write-Host "   建议使用浏览器打开: test-websocket.html" -ForegroundColor Gray
Write-Host ""

Write-Host "=== 诊断建议 ===" -ForegroundColor Cyan
Write-Host "如果服务正在运行但第二条消息未显示，请检查：" -ForegroundColor White
Write-Host "  1. 浏览器开发者工具 (F12) -> Network -> WS 查看 WebSocket 消息"
Write-Host "  2. 浏览器控制台 (F12) -> Console 查看 JavaScript 错误"
Write-Host "  3. zeroclaw-core 和 zeroclaw-edge 的控制台输出"
Write-Host ""
Write-Host "常见问题：" -ForegroundColor White
Write-Host "  - 第一条消息处理时间过长（>30秒）导致会话锁超时"
Write-Host "  - WebSocket 连接在第一条消息后断开"
Write-Host "  - 前端 typing 状态未正确重置"
Write-Host ""
