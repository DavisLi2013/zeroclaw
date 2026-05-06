# ZeroClaw Core 启动脚本
# 设置OpenAI API密钥环境变量并启动core服务

# 设置OpenAI API Key
$env:OPENAI_API_KEY = "sk-9mYjfdrGia3EOoMGu2i6iEpNphM0gxcVhjSmcKZtXGgxYkWm"

# 运行ZeroClaw Core
& ".\target\release\zeroclaw-core.exe" --host 127.0.0.1 --port 42618 $args
