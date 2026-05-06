# ZeroClaw 完整启动脚本
# 设置OpenAI API密钥环境变量并启动所有服务

# 设置OpenAI API Key
$env:OPENAI_API_KEY = "sk-9mYjfdrGia3EOoMGu2i6iEpNphM0gxcVhjSmcKZtXGgxYkWm"

# 停止现有进程
Get-Process | Where-Object {$_.ProcessName -like "*zeroclaw*"} | Stop-Process -Force -ErrorAction SilentlyContinue

# 启动Core服务 (后台)
Start-Process -FilePath ".\target\release\zeroclaw-core.exe" -ArgumentList "--host 127.0.0.1 --port 42618 $args" -PassThru

# 等待Core服务启动
Start-Sleep -Seconds 3

# 启动Gateway服务
& ".\target\release\zeroclaw.exe" gateway start $args
