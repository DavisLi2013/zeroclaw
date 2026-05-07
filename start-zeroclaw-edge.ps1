# ZeroClaw Edge 启动脚本
# 设置OpenAI API密钥环境变量并启动edge服务

# 设置OpenAI API Key
$env:OPENAI_API_KEY = "sk-9mYjfdrGia3EOoMGu2i6iEpNphM0gxcVhjSmcKZtXGgxYkWm"

# 设置Anthropic API Key（用于anthropic provider）
$env:ANTHROPIC_API_KEY = "sk-uztpxlIQmdAZxPRfBivPKvniLUAKe68VivsD5sIIqRVcLG4i"

if (-not $env:ZEROCLAW_CORE_TOKEN) {
    $env:ZEROCLAW_CORE_TOKEN = "zc_local_core_dev_token"
}

# 禁用pairing认证（本地开发环境）
$env:ZEROCLAW_REQUIRE_PAIRING = "false"

# 启用详细日志
$env:RUST_LOG = "debug,zeroclaw=trace"

# 运行ZeroClaw Edge
& ".\target\release\zeroclaw-edge.exe" --host 127.0.0.1 --port 42617 --core-grpc http://127.0.0.1:42618 --core-token $env:ZEROCLAW_CORE_TOKEN $args
