# ZeroClaw Core 启动脚本
# 设置OpenAI API密钥环境变量并启动core服务

# 设置OpenAI API Key
$env:OPENAI_API_KEY = "sk-9mYjfdrGia3EOoMGu2i6iEpNphM0gxcVhjSmcKZtXGgxYkWm"

if (-not $env:ZEROCLAW_CORE_TOKEN) {
    $env:ZEROCLAW_CORE_TOKEN = "zc_local_core_dev_token"
}

# 运行ZeroClaw Core
& ".\target\release\zeroclaw-core.exe" --host 127.0.0.1 --port 42618 --core-token $env:ZEROCLAW_CORE_TOKEN $args
