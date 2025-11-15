#!/bin/bash
set -e

echo "=========================================="
echo "Violet IM Docker 部署脚本"
echo "=========================================="

# 检查 Docker 和 Docker Compose
if ! command -v docker &> /dev/null; then
    echo "错误: 未找到 Docker，请先安装 Docker"
    exit 1
fi

if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    echo "错误: 未找到 Docker Compose，请先安装 Docker Compose"
    exit 1
fi

# 创建必要的目录
echo "创建必要的目录..."
mkdir -p mqtt/data mqtt/log mysql/init uploads

# 检查 .env 文件
if [ ! -f .env ]; then
    echo "警告: 未找到 .env 文件，使用默认配置"
    echo "建议: 复制 .env.example 为 .env 并修改配置"
    if [ -f .env.example ]; then
        read -p "是否复制 .env.example 为 .env? (y/n) " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            cp .env.example .env
            echo "已创建 .env 文件，请根据需要修改配置"
        fi
    fi
fi

# 构建并启动服务
echo ""
echo "构建 Docker 镜像..."
docker-compose build

echo ""
echo "启动服务..."
docker-compose up -d

echo ""
echo "等待服务启动..."
sleep 5

# 检查服务状态
echo ""
echo "服务状态:"
docker-compose ps

echo ""
echo "=========================================="
echo "部署完成！"
echo "=========================================="
echo ""
echo "服务地址:"
echo "  - IM Server API: http://localhost:${IM_SERVER_PORT:-3000}"
echo "  - IM Connect WebSocket: ws://localhost:${IM_CONNECT_PORT:-3001}"
echo ""
echo "常用命令:"
echo "  - 查看日志: docker-compose logs -f"
echo "  - 停止服务: docker-compose stop"
echo "  - 重启服务: docker-compose restart"
echo "  - 查看状态: docker-compose ps"
echo ""
echo "详细文档请查看 DEPLOYMENT.md"

