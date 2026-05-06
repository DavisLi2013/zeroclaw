# ZeroClaw Edge 启动脚本
# 设置OpenAI API密钥环境变量并启动edge服务

# 设置OpenAI API Key
$env:OPENAI_API_KEY = "sk-9mYjfdrGia3EOoMGu2i6iEpNphM0gxcVhjSmcKZtXGgxYkWm"

# 运行ZeroClaw Edge
& ".\target\release\zeroclaw-edge.exe" --host 127.0.0.1 --port 42617 --core-grpc http://127.0.0.1:42618 $args
