# 多阶段构建 - 构建阶段
FROM rust:1.75-slim as builder

# 安装构建依赖
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 设置工作目录
WORKDIR /app

# 复制 Cargo 文件
COPY Cargo.toml Cargo.lock ./
COPY im-share/Cargo.toml ./im-share/
COPY im-server/Cargo.toml ./im-server/

# 创建虚拟清单文件以缓存依赖
RUN mkdir -p im-share/src im-server/src && \
    echo "fn main() {}" > im-share/src/lib.rs && \
    echo "fn main() {}" > im-server/src/main.rs

# 构建依赖（利用 Docker 缓存）
RUN cargo build --release --package im-server && \
    rm -rf im-share/src im-server/src

# 复制源代码
COPY im-share ./im-share
COPY im-server ./im-server

# 构建应用
RUN cargo build --release --package im-server

# 运行阶段
FROM debian:bookworm-slim

# 安装运行时依赖
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    bash \
    && rm -rf /var/lib/apt/lists/*

# 创建应用用户
RUN useradd -m -u 1000 appuser

# 设置工作目录
WORKDIR /app

# 从构建阶段复制二进制文件
COPY --from=builder /app/target/release/im-server /app/im-server

# 复制启动脚本
COPY im-server/docker-entrypoint.sh /app/docker-entrypoint.sh

# 创建必要的目录
RUN mkdir -p /app/uploads && \
    chmod +x /app/docker-entrypoint.sh && \
    chown -R appuser:appuser /app

# 切换到非root用户
USER appuser

# 暴露端口
EXPOSE 3000

# 设置入口点
ENTRYPOINT ["/app/docker-entrypoint.sh"]

# 运行应用
CMD ["./im-server"]

