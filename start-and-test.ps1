# 完整的测试流程脚本

Write-Host "=== ZeroClaw 完整测试流程 ===" -ForegroundColor Cyan
Write-Host ""

# 步骤 1: 启动 Core 服务
Write-Host "步骤 1: 启动 zeroclaw-core..." -ForegroundColor Yellow
Start-Process powershell -ArgumentList "-ExecutionPolicy Bypass -NoExit -Command `"cd 'D:\workspace\axi-research\zeroclaw'; .\start-zeroclaw-core.ps1 --project-root 'D:\workspace\axi-research\zeroclaw'`"" -WindowStyle Normal

Write-Host "等待 Core 服务启动..." -ForegroundColor Gray
Start-Sleep -Seconds 5

# 步骤 2: 启动 Edge 服务
Write-Host "步骤 2: 启动 zeroclaw-edge..." -ForegroundColor Yellow
Start-Process powershell -ArgumentList "-ExecutionPolicy Bypass -NoExit -Command `"cd 'D:\workspace\axi-research\zeroclaw'; .\start-zeroclaw-edge.ps1 --project-root 'D:\workspace\axi-research\zeroclaw'`"" -WindowStyle Normal

Write-Host "等待 Edge 服务启动..." -ForegroundColor Gray
Start-Sleep -Seconds 5

# 步骤 3: 检查服务状态
Write-Host "步骤 3: 检查服务状态..." -ForegroundColor Yellow
$coreProcess = Get-Process -Name "zeroclaw-core" -ErrorAction SilentlyContinue
$edgeProcess = Get-Process -Name "zeroclaw-edge" -ErrorAction SilentlyContinue

if ($coreProcess) {
    Write-Host "   ✓ zeroclaw-core 已启动 (PID: $($coreProcess.Id))" -ForegroundColor Green
} else {
    Write-Host "   ✗ zeroclaw-core 启动失败" -ForegroundColor Red
    exit 1
}

if ($edgeProcess) {
    Write-Host "   ✓ zeroclaw-edge 已启动 (PID: $($edgeProcess.Id))" -ForegroundColor Green
} else {
    Write-Host "   ✗ zeroclaw-edge 启动失败" -ForegroundColor Red
    exit 1
}

# 步骤 4: 测试健康检查
Write-Host "步骤 4: 测试健康检查..." -ForegroundColor Yellow
Start-Sleep -Seconds 2
try {
    $response = Invoke-WebRequest -Uri "http://127.0.0.1:42617/health" -TimeoutSec 5 -ErrorAction Stop
    Write-Host "   ✓ Edge 健康检查成功: $($response.StatusCode)" -ForegroundColor Green
} catch {
    Write-Host "   ✗ Edge 健康检查失败: $($_.Exception.Message)" -ForegroundColor Red
}

Write-Host ""
Write-Host "=== 服务已启动 ===" -ForegroundColor Green
Write-Host ""
Write-Host "现在请：" -ForegroundColor Cyan
Write-Host "  1. 用浏览器打开: D:\workspace\axi-research\zeroclaw\test-websocket.html"
Write-Host "  2. 发送第一条消息，等待完整响应"
Write-Host "  3. 发送第二条消息，观察是否有响应"
Write-Host "  4. 将诊断工具中的所有日志复制给我"
Write-Host ""
Write-Host "同时观察 zeroclaw-core 和 zeroclaw-edge 的控制台窗口输出" -ForegroundColor Yellow
Write-Host ""
