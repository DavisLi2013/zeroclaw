# ZeroClaw 启动脚本
# 设置API密钥环境变量
$env:ZEROCLAW_API_KEY = "sk-9mYjfdrGia3EOoMGu2i6iEpNphM0gxcVhjSmcKZtXGgxYkWm"

# 运行ZeroClaw
& ".\target\release\zeroclaw.exe" $args
