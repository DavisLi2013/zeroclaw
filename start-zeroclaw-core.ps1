# ZeroClaw Core 启动脚本
# 设置OpenAI API密钥环境变量并启动core服务

# 设置OpenAI API Key
# $env:OPENAI_API_KEY = "sk-9mYjfdrGia3EOoMGu2i6iEpNphM0gxcVhjSmcKZtXGgxYkWm"

# 公司claude 中转key
$env:OPENAI_API_KEY = "sk-uztpxlIQmdAZxPRfBivPKvniLUAKe68VivsD5sIIqRVcLG4i"

if (-not $env:ZEROCLAW_CORE_TOKEN) {
    $env:ZEROCLAW_CORE_TOKEN = "zc_local_core_dev_token"
}

# 启用详细日志
$env:RUST_LOG = "debug,zeroclaw=trace"

# 运行ZeroClaw Core
& ".\target\release\zeroclaw-core.exe" --host 127.0.0.1 --port 42618 --core-token $env:ZEROCLAW_CORE_TOKEN $args
