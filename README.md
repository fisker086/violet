# 紫罗兰IM - Violet IM

> 紫罗兰（Violet）象征着永恒的美与爱，代表着忠诚与信任。正如紫罗兰的花语，我们致力于构建一个安全、可靠、高性能的即时通讯系统，让每一次沟通都如紫罗兰般优雅而持久。

## 📖 项目简介

紫罗兰IM（Violet IM）是一个基于 Rust 开发的高性能分布式即时通讯系统，采用微服务架构设计，提供完整的即时通讯功能，包括单聊、群聊、音视频通话等。

### 项目寓意

- **紫罗兰（Violet）**：象征着永恒的美与爱，代表着忠诚与信任
- **设计理念**：追求优雅、安全、可靠的即时通讯体验
- **技术追求**：高性能、低延迟、高并发

## ✨ 功能特性

- ✅ 用户注册、登录、认证（JWT）
- ✅ 单聊消息（文本、图片、文件等）
- ✅ 群聊消息（支持 @ 提醒）
- ✅ 好友管理（添加、删除、备注、黑名单）
- ✅ 群组管理（创建、加入、退出、解散、权限管理）
- ✅ 消息已读状态
- ✅ 文件上传下载（支持图片压缩和缩略图）
- ✅ WebRTC 音视频通话
- ✅ 实时消息推送（WebSocket + MQTT）
- ✅ 聊天会话管理（置顶、免打扰、备注）

## 🏗️ 系统架构

### 架构图

```
┌─────────────────────────────────────────────────────────┐
│                      Nginx (80/443)                      │
│                   反向代理统一入口                        │
└──────────────┬──────────────────────────────────────────┘
               │
    ┌──────────┴──────────┐
    │                     │
┌───▼──────┐      ┌───────▼──────┐
│im-server │      │  im-connect  │
│(HTTP API)│      │ (WebSocket)  │
│  :3000   │      │    :3001     │
└───┬──────┘      └───────┬──────┘
    │                     │
    │  ┌──────────────────┘
    │  │
    │  └──────────┬──────────┐
    │             │          │
┌───▼──────┐  ┌───▼──────┐  ┌───▼──────┐      ┌──────────┐
│  MySQL   │  │  Redis   │  │   MQTT   │      │   SRS    │
│  :3306   │  │  :6379    │  │  :1883   │      │ (WebRTC) │
└──────────┘  └───────────┘  └──────────┘      │  :8000    │
                                               │  :1985    │
                                               └─────┬─────┘
                                                     │
                                          ┌──────────┘
                                          │
                                    (im-server 调用)
                                    HTTP API 中转
                                    
说明：
- 客户端 → Nginx → im-server/im-connect
- im-server → SRS (HTTP API) → WebRTC 连接
- 客户端不直接连接 SRS，通过 im-server 中转
```

### 核心组件

- **im-server**: HTTP API 服务，提供 RESTful API 处理业务逻辑
- **im-connect**: WebSocket 服务，处理实时消息推送
- **im-share**: 共享库，包含通用功能模块（认证、用户、群组、MQTT、Redis、雪花算法等）
- **MySQL**: 数据存储（用户、消息、群组、好友关系等）
- **Redis**: 缓存和会话管理
- **MQTT**: 消息队列和推送（Mosquitto）
- **SRS**: WebRTC 音视频服务（Simple Realtime Server）
  - **重要**：客户端不直接连接 SRS，而是通过 `im-server` 中转
  - `im-server` 调用 SRS HTTP API 进行 WebRTC SDP 交换
  - 客户端通过 `im-server` 获取 WebRTC 连接信息
- **Nginx**: 反向代理，统一对外提供服务

### 技术栈

- **后端语言**: Rust (Edition 2024)
- **Web 框架**: Axum 0.8.6
- **异步运行时**: Tokio 1.48.0
- **数据库**: MySQL 8.0
- **ORM**: SQLx 0.8.6
- **缓存**: Redis 7
- **消息队列**: MQTT (Mosquitto 2.0)
- **音视频**: SRS 6.0
- **反向代理**: Nginx
- **容器化**: Docker & Docker Compose

## 📁 项目结构

```
violet/
├── im-server/          # HTTP API 服务
│   ├── src/
│   │   ├── handlers/   # 请求处理器
│   │   ├── service/    # 业务逻辑层
│   │   ├── model/      # 数据模型
│   │   ├── routes/     # 路由定义
│   │   ├── middleware/ # 中间件（认证等）
│   │   ├── db.rs       # 数据库连接
│   │   └── main.rs     # 入口文件
│   ├── Dockerfile
│   ├── config.toml     # 配置文件
│   └── Cargo.toml
├── im-connect/         # WebSocket 服务
│   ├── src/
│   │   ├── handlers/   # WebSocket 处理器
│   │   ├── routes/     # 路由定义
│   │   └── main.rs     # 入口文件
│   ├── Dockerfile
│   ├── config.toml     # 配置文件
│   └── Cargo.toml
├── im-share/           # 共享库
│   ├── src/
│   │   ├── auth.rs     # 认证模块
│   │   ├── user.rs     # 用户相关
│   │   ├── group/      # 群组相关
│   │   ├── mqtt.rs     # MQTT 客户端
│   │   ├── redis.rs    # Redis 客户端
│   │   ├── snowflake.rs # 雪花算法 ID 生成
│   │   └── lib.rs
│   └── Cargo.toml
├── sql/                # 数据库脚本
│   └── violet_table.sql # 数据库表结构
├── nginx/              # Nginx 配置
│   └── nginx.conf
├── mqtt/               # MQTT 配置
│   └── mosquitto.conf
├── srs/                # SRS 配置
│   └── conf/
│       └── docker.conf
├── uploads/            # 文件上传目录
├── docker-compose.yml  # Docker Compose 配置
├── Cargo.toml          # Workspace 配置
└── README.md
```

## 🚀 快速开始

### 前置要求

- **操作系统**: Linux / macOS / Windows (WSL2)
- **Docker**: 20.10+ 
- **Docker Compose**: 2.0+
- **Rust**: 1.70+ (仅本地开发需要)
- **内存**: 至少 4GB 可用内存
- **磁盘**: 至少 10GB 可用磁盘空间

### 方式一：Docker Compose 一键部署

Docker Compose 方式适合快速部署和开发环境，可以一键启动所有服务。

#### 1. 创建网络（如果不存在）

```bash
docker network create violet-network
```

#### 2. 配置环境变量（可选）

创建 `.env` 文件（可选，使用默认值）：

```bash
# MySQL 配置
MYSQL_ROOT_PASSWORD=123456
MYSQL_DATABASE=violet
MYSQL_USER=violet
MYSQL_PASSWORD=violet123
MYSQL_PORT=3306

# Redis 配置
REDIS_PASSWORD=
REDIS_PORT=6379
REDIS_DB=0

# MQTT 配置
MQTT_PORT=1883
MQTT_WS_PORT=9001

# 服务端口
IM_SERVER_PORT=3000
IM_CONNECT_PORT=3001

# SRS 配置
SRS_RTMP_PORT=1935
SRS_HTTP_API_PORT=1985
SRS_HTTP_PORT=8080
SRS_HTTPS_PORT=1990
SRS_HTTP_API_SSL_PORT=8088
SRS_WEBRTC_PORT=8000
# ⚠️ 重要：SRS_CANDIDATE 必须设置为物理机的实际 IP 地址（不是 127.0.0.1）
# 这个 IP 用于 WebRTC 的 ICE 候选地址，客户端需要通过这个 IP 建立 WebRTC 连接
# 查看物理机 IP：Linux/macOS 使用 ifconfig，Windows 使用 ipconfig
SRS_CANDIDATE=127.0.0.1  # 替换为你的物理机实际 IP 地址

# Nginx 配置
NGINX_HTTP_PORT=80
NGINX_HTTPS_PORT=443

# JWT 配置（重要：im-server 和 im-connect 必须使用相同的密钥）
# 默认值已设置为随机生成的密钥，生产环境建议修改为更安全的密钥
JWT_SECRET=337eb69ef604dec5cdb04481242877fea7db31e4c1fd236497033431ab41d499
JWT_EXPIRATION_HOURS=24

# 日志级别
RUST_LOG=info
```

#### 3. 配置 SRS CANDIDATE（重要）

在启动服务前，需要设置 SRS 的 CANDIDATE 环境变量为物理机的实际 IP 地址：

```bash
# 查看物理机 IP 地址
# Linux/macOS:
ifconfig | grep "inet " | grep -v 127.0.0.1

# Windows:
ipconfig

# 然后在 .env 文件中设置（或直接修改 docker-compose.yml）
# SRS_CANDIDATE=你的物理机IP，例如：SRS_CANDIDATE=127.0.0.1
```

> **重要**：CANDIDATE 必须设置为**物理机的实际 IP 地址**（不是 127.0.0.1），用于 WebRTC 的 ICE 候选地址。客户端需要通过这个 IP 建立 WebRTC UDP 连接。

#### 4. 启动所有服务

```bash
# 启动所有服务（包括 MySQL、Redis、MQTT、SRS、im-server、im-connect、nginx）
docker-compose up -d

# 查看服务状态
docker-compose ps

# 查看日志
docker-compose logs -f

# 查看特定服务日志
docker-compose logs -f im-server
docker-compose logs -f im-connect
```

#### 5. 导入数据库

```bash
# 等待 MySQL 启动完成后，导入数据库
docker exec -i violet-mysql mysql -uroot -p123456 violet < sql/violet_table.sql
```

#### 6. 验证部署

```bash
# 检查 Nginx
curl http://localhost/health

# 检查 im-server API
curl http://localhost/api/im/users
```

---

### 方式二：单独 Docker 命令部署（推荐生产环境）

单独 Docker 命令方式适合生产环境，可以更灵活地控制每个服务的配置和部署。

#### 1. 启动 MySQL

```bash
docker run -d \
  --name violet-mysql \
  -e MYSQL_ROOT_PASSWORD=123456 \
  -e MYSQL_DATABASE=violet \
  -e MYSQL_USER=violet \
  -e MYSQL_PASSWORD=violet123 \
  -p 3306:3306 \
  mysql:8.0
```

#### 2. 启动 Redis

```bash
docker run -d \
  --name violet-redis \
  -p 6379:6379 \
  redis:latest
```

#### 3. 启动 MQTT (Mosquitto)

```bash
docker run -d \
  --name violet-mqtt \
  -p 1883:1883 \
  -p 9001:9001 \
  eclipse-mosquitto:2.0
```

#### 4. 启动 SRS (Simple Realtime Server)

```bash
# ⚠️ 重要：CANDIDATE 必须设置为物理机的实际 IP 地址（不是 127.0.0.1）
# 例如：如果物理机 IP 是 192.168.1.9，则设置为 --env CANDIDATE=192.168.1.9
# 这个 IP 用于 WebRTC 的 ICE 候选地址，客户端需要通过这个 IP 建立 WebRTC 连接
docker run -d \
  --name violet-srs \
  -p 1935:1935 \
  -p 1985:1985 \
  -p 8080:8080 \
  -p 1990:1990 \
  -p 8088:8088 \
  -p 8000:8000/udp \
  --env CANDIDATE=127.0.0.1 \
  registry.cn-hangzhou.aliyuncs.com/ossrs/srs:6.0-d2
```

> **重要说明**：
> - SRS 服务主要用于 WebRTC 音视频通话。客户端**不会直接连接 SRS 服务**，而是通过 `im-server` 中转。`im-server` 会调用 SRS 的 HTTP API 进行 WebRTC SDP 交换，客户端通过 `im-server` 获取 WebRTC 连接信息。
> - **CANDIDATE 环境变量**：必须设置为**物理机的实际 IP 地址**（不是容器 IP 或 127.0.0.1），用于 WebRTC 的 ICE 候选地址。客户端需要通过这个 IP 建立 WebRTC UDP 连接。
> - 查看物理机 IP 的方法：
>   - Linux/macOS: `ifconfig` 或 `ip addr`
>   - Windows: `ipconfig`

#### 5. 导入数据库

```bash
# 等待 MySQL 启动完成后，导入数据库
docker exec -i violet-mysql mysql -uroot -p123456 violet < sql/violet_table.sql

# 或者使用 mysql 客户端
mysql -h127.0.0.1 -uroot -p123456 violet < sql/violet_table.sql
```

#### 6. 构建 im-server 和 im-connect 镜像

```bash
# 在项目根目录执行
cd im-server
docker build -t violet-im-server:latest -f Dockerfile ..

cd ../im-connect
docker build -t violet-im-connect:latest -f Dockerfile ..
```

#### 7. 启动 im-server

```bash
# 注意：不使用 Docker 网络，所有服务通过 localhost 通信
# MYSQL_HOST、REDIS_HOST、MQTT_HOST、SRS_HOST 使用 127.0.0.1 或 localhost
docker run -d \
  --name violet-im-server \
  -p 3000:3000 \
  -e RUST_LOG=info \
  -e MYSQL_HOST=127.0.0.1 \
  -e MYSQL_PORT=3306 \
  -e MYSQL_USER=violet \
  -e MYSQL_PASSWORD=violet123 \
  -e MYSQL_DATABASE=violet \
  -e REDIS_HOST=127.0.0.1 \
  -e REDIS_PORT=6379 \
  -e REDIS_DB=0 \
  -e MQTT_HOST=127.0.0.1 \
  -e MQTT_PORT=1883 \
  -e SERVER_PORT=3000 \
  -e JWT_SECRET=337eb69ef604dec5cdb04481242877fea7db31e4c1fd236497033431ab41d499 \
  -e JWT_EXPIRATION_HOURS=24 \
  -e SRS_HOST=http://127.0.0.1:1985 \
  -e SRS_HTTP_HOST=http://127.0.0.1:8080 \
  -e SRS_WEBRTC_PORT=8000 \
  -e SRS_APP=live \
  -e SRS_CLIENT_HOST=http://127.0.0.1:1985 \
  -e SRS_CLIENT_HTTP_HOST=http://127.0.0.1:8080 \
  -v $(pwd)/uploads:/app/uploads \
  violet-im-server:latest
```

#### 8. 启动 im-connect

```bash
# 注意：不使用 Docker 网络，所有服务通过 localhost 通信
# REDIS_HOST、MQTT_HOST 使用 127.0.0.1 或 localhost
docker run -d \
  --name violet-im-connect \
  -p 3001:3001 \
  -e RUST_LOG=info \
  -e REDIS_HOST=127.0.0.1 \
  -e REDIS_PORT=6379 \
  -e REDIS_DB=0 \
  -e MQTT_HOST=127.0.0.1 \
  -e MQTT_PORT=1883 \
  -e CONNECT_PORT=3001 \
  -e JWT_SECRET=337eb69ef604dec5cdb04481242877fea7db31e4c1fd236497033431ab41d499 \
  -e JWT_EXPIRATION_HOURS=24 \
  violet-im-connect:latest
```

#### 9. 启动 Nginx

```bash
# 注意：不使用 Docker 网络，Nginx 通过 localhost 访问服务
# 需要修改 nginx.conf 中的 upstream 配置，使用 127.0.0.1 或 host.docker.internal
docker run -d \
  --name violet-nginx \
  -p 80:80 \
  -p 443:443 \
  -v $(pwd)/nginx/nginx.conf:/etc/nginx/conf.d/default.conf:ro \
  nginx:alpine
```

> **重要**：nginx.conf 中的 upstream 配置说明：
> - **Docker Compose 方式**：使用服务名（`im-server`、`im-connect`），docker-compose 会自动解析
> - **单独 Docker 命令方式**：需要使用 `host.docker.internal` 或 `127.0.0.1`（在 Linux 上可能需要使用 `--network host`）
> 
> 如果使用单独 Docker 命令部署，需要修改 `nginx/nginx.conf` 中的 upstream 配置：
> ```nginx
> upstream im_server {
>     server host.docker.internal:3000;  # macOS/Windows 使用 host.docker.internal
>     # server 127.0.0.1:3000;  # Linux 可能需要使用 --network host
> }
> 
> upstream im_connect {
>     server host.docker.internal:3001;  # macOS/Windows 使用 host.docker.internal
>     # server 127.0.0.1:3001;  # Linux 可能需要使用 --network host
> }
> ```
> 
> 或者使用 `--network host` 模式（仅 Linux）：
> ```bash
> docker run -d \
>   --name violet-nginx \
>   --network host \
>   -v $(pwd)/nginx/nginx.conf:/etc/nginx/conf.d/default.conf:ro \
>   nginx:alpine
> ```
> 这样可以直接使用 `127.0.0.1:3000` 和 `127.0.0.1:3001`

#### 10. 验证部署

```bash
# 检查所有容器状态
docker ps | grep violet

# 检查 Nginx
curl http://localhost/health

# 检查 im-server API
curl http://localhost/api/im/users

# 查看服务日志
docker logs violet-im-server
docker logs violet-im-connect
docker logs violet-nginx
```

#### 11. 服务管理命令

```bash
# 停止服务
docker stop violet-nginx violet-im-connect violet-im-server violet-srs violet-mqtt violet-redis violet-mysql

# 启动服务
docker start violet-mysql violet-redis violet-mqtt violet-srs violet-im-server violet-im-connect violet-nginx

# 删除服务（谨慎使用）
docker rm -f violet-nginx violet-im-connect violet-im-server violet-srs violet-mqtt violet-redis violet-mysql

# 删除数据卷（谨慎使用，会删除所有数据）
docker volume rm mysql_data redis_data
```

---

### 方式三：本地开发部署（不使用 Docker）

#### 1. 安装 Rust

**Linux / macOS:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

**Windows:**
下载并运行 [rustup-init.exe](https://rustup.rs/)

**验证安装:**
```bash
rustc --version
cargo --version
```

#### 2. 启动依赖服务

```bash
# 使用 Docker Compose 启动 MySQL、Redis、MQTT
docker-compose up -d mysql redis mqtt

# 或者使用上面的单独安装命令
```

#### 3. 导入数据库

```bash
# 使用 mysql 客户端导入
mysql -h127.0.0.1 -uroot -p123456 violet < sql/violet_table.sql
```

#### 4. 配置服务

编辑配置文件：

**im-server/config.toml:**
```toml
[database]
host = "127.0.0.1"
port = 3306
user = "root"
password = "123456"
database = "violet"

[redis]
host = "127.0.0.1"
port = 6379
db = 0

[mqtt]
host = "127.0.0.1"
port = 1883

[jwt]
secret = "your-secret-key-change-in-production"
expiration_hours = 24
```

**im-connect/config.toml:**
```toml
[redis]
host = "127.0.0.1"
port = 6379
db = 0

[mqtt]
host = "127.0.0.1"
port = 1883

[jwt]
secret = "your-secret-key-change-in-production"
expiration_hours = 24
```

#### 5. 编译项目

```bash
# 在项目根目录（violet/）编译整个 workspace
cargo build --release

# 或者单独编译各个服务
cd im-server
cargo build --release

cd ../im-connect
cargo build --release
```

#### 6. 启动服务

**启动 im-server:**
```bash
cd im-server
cargo run --release

# 或者使用编译好的二进制文件
./target/release/im-server
```

**启动 im-connect:**
```bash
cd im-connect
cargo run --release

# 或者使用编译好的二进制文件
./target/release/im-connect
```

**启动 Nginx（可选，用于反向代理）:**
```bash
docker-compose up -d nginx
```

## 🔧 开发指南

### 本地开发环境

1. **安装 Rust 工具链**
   ```bash
   rustup install stable
   rustup default stable
   ```

2. **安装开发工具**
   ```bash
   # Rust 格式化工具
   rustup component add rustfmt
   
   # Rust 代码检查工具
   rustup component add clippy
   ```

3. **启动依赖服务**
   ```bash
   docker-compose up -d mysql redis mqtt
   ```

4. **运行开发模式**
   ```bash
   # 终端 1: 运行 im-server
   cd im-server
   cargo run
   
   # 终端 2: 运行 im-connect
   cd im-connect
   cargo run
   ```

### 代码格式化

```bash
# 格式化所有代码
cargo fmt

# 检查代码风格
cargo clippy
```

### 构建 Docker 镜像

```bash
# 构建所有服务
docker-compose build

# 构建特定服务
docker-compose build im-server
docker-compose build im-connect
```

## 📡 服务端口说明

### 对外端口（通过 Nginx 统一代理）

- **80**: HTTP 服务（所有 API 和 WebSocket）
- **443**: HTTPS 服务（需配置 SSL 证书）

### 内部服务端口（容器间通信）

- **im-server**: 3000
- **im-connect**: 3001
- **MySQL**: 3306
- **Redis**: 6379
- **MQTT**: 1883 (TCP), 9001 (WebSocket)
- **SRS**: 
  - 1935 (RTMP)
  - 1985 (HTTP API)
  - 8080 (HTTP)
  - 8000 (WebRTC UDP)

### API 路径说明

所有 API 通过 Nginx 统一代理，路径规则如下：

- `/api/*` → im-server:3000 (RESTful API，包括所有业务逻辑和 WebRTC 相关 API)
- `/ws/*` → im-connect:3001 (WebSocket)
- `/uploads/*` → im-server:3000 (文件服务)

> **重要说明**：
> - 客户端**不直接连接 SRS 或 MQTT**，所有请求都通过 `im-server` 中转
> - `im-server` 内部会调用 SRS 的 HTTP API 进行 WebRTC 相关操作
> - Nginx 不直接代理 SRS 或 MQTT，只代理 `im-server` 和 `im-connect`

## 🔧 Nginx 配置说明

### 配置文件位置

Nginx 配置文件位于 `nginx/nginx.conf`，主要配置如下：

### 核心配置说明

1. **上游服务定义**
   ```nginx
   upstream im_server {
       server im-server:3000;
   }
   
   upstream im_connect {
       server im-connect:3001;
   }
   ```
   > **注意**：Nginx 不直接代理 SRS 或 MQTT，这些服务由 `im-server` 内部调用

2. **API 代理** (`/api/*`)
   - 代理所有 RESTful API 请求到 `im-server:3000`
   - 包括所有业务逻辑、WebRTC 相关 API（通过 `im-server` 中转调用 SRS）
   - 设置必要的代理头（Host、X-Real-IP、X-Forwarded-For 等）
   - 超时设置：60 秒

3. **WebSocket 代理** (`/ws/*`)
   - 代理 WebSocket 连接到 `im-connect:3001`
   - 设置 WebSocket 必需的头（Upgrade、Connection）
   - 超时设置：7 天（保持长连接）

4. **文件服务** (`/uploads/*`)
   - 将 `/uploads/` 重写为 `/api/upload/`
   - 代理到 `im-server:3000`
   - 关闭缓冲和缓存，支持大文件上传
   - 超时设置：300 秒

5. **SRS 和 MQTT 说明**
   - **不通过 Nginx 直接代理**
   - 客户端通过 `im-server` 的 API 进行 WebRTC 通话
   - `im-server` 内部会调用 SRS 的 HTTP API（直接连接，不经过 Nginx）
   - MQTT 也由 `im-server` 和 `im-connect` 内部使用，不对外暴露

### 完整配置文件

完整的 Nginx 配置文件请查看 `nginx/nginx.conf`，包含：
- 所有代理规则（仅 `im-server` 和 `im-connect`）
- 健康检查端点
- HTTPS 配置示例（注释状态）


### 生产环境配置建议

1. **启用 HTTPS**
   - 取消注释 HTTPS server 配置
   - 配置 SSL 证书路径
   - 配置 HTTP 到 HTTPS 的重定向

2. **安全加固**
   - 限制请求大小：`client_max_body_size`
   - 配置访问日志和错误日志
   - 所有 API 请求都通过 `im-server`，由 `im-server` 统一处理认证和授权

3. **性能优化**
   - 调整 worker 进程数
   - 启用 gzip 压缩
   - 配置缓存策略（如需要）

## 📚 API 文档

### 用户相关

- `POST /api/im/users` - 用户注册
- `POST /api/im/auth/login` - 用户登录
- `GET /api/im/users/{user_id}` - 获取用户信息
- `PUT /api/im/users/{user_id}` - 更新用户信息

### 消息相关

- `POST /api/im/messages/single` - 发送单聊消息
- `GET /api/im/messages/single` - 获取单聊消息列表
- `POST /api/im/messages/group` - 发送群聊消息
- `GET /api/im/messages/group/{group_id}` - 获取群聊消息列表

### 好友相关

- `POST /api/im/friends` - 添加好友
- `GET /api/im/friends` - 获取好友列表
- `DELETE /api/im/friends/{friend_id}` - 删除好友
- `PUT /api/im/friends/{friend_id}/remark` - 修改好友备注

### 群组相关

- `POST /api/im/groups` - 创建群组
- `GET /api/im/groups` - 获取群组列表
- `GET /api/im/groups/{group_id}` - 获取群组详情
- `POST /api/im/groups/{group_id}/members` - 添加群成员
- `DELETE /api/im/groups/{group_id}/members/{member_id}` - 移除群成员

### 文件相关

- `POST /api/upload` - 上传文件
- `GET /api/upload/{*path}` - 下载文件

### WebSocket

- `ws://localhost/ws/connect` - WebSocket 连接端点

详细 API 文档请参考代码中的路由定义。

## 🔒 生产环境部署

### 1. 安全配置

**重要：生产环境必须修改以下配置**

1. **修改所有默认密码**
   - MySQL root 密码
   - MySQL 用户密码
   - Redis 密码（如需要）
   - JWT 密钥（必须足够复杂）

2. **配置 SSL 证书**
   - 在 `nginx/nginx.conf` 中配置 HTTPS
   - 将证书文件挂载到 Nginx 容器

3. **限制端口暴露**
   - 注释掉 `docker-compose.yml` 中的直接端口映射
   - 所有服务仅通过 Nginx 对外提供服务

4. **配置防火墙**
   - 仅开放 80、443 端口
   - WebRTC UDP 端口（8000）需要开放

### 2. 性能优化

1. **数据库优化**
   - 配置 MySQL 连接池大小
   - 启用查询缓存
   - 配置合适的索引
   - 定期优化表结构

2. **Redis 优化**
   - 配置内存限制
   - 启用持久化（AOF）
   - 配置合适的过期策略

3. **Nginx 优化**
   - 配置 worker 进程数
   - 启用 gzip 压缩
   - 配置缓存策略
   - 限制请求大小

### 3. 监控和日志

1. **日志管理**
   - 配置日志轮转
   - 集中日志收集（如 ELK、Loki）
   - 设置日志级别

2. **监控**
   - 配置健康检查
   - 监控服务资源使用（CPU、内存、磁盘）
   - 配置告警（Prometheus + Grafana）

### 4. 备份策略

1. **数据库备份**
   ```bash
   # 定期备份 MySQL
   docker exec violet-mysql mysqldump -u root -p violet > backup_$(date +%Y%m%d).sql
   
   # 恢复数据库
   docker exec -i violet-mysql mysql -u root -p violet < backup_20240101.sql
   ```

2. **文件备份**
   - 定期备份 `uploads/` 目录
   - 考虑使用对象存储（OSS/S3）

## ❓ 常见问题

### 1. 服务启动失败

- 检查端口是否被占用：`lsof -i :3000` 或 `netstat -tulpn | grep 3000`
- 查看服务日志：`docker-compose logs [service_name]`
- 检查环境变量配置
- 检查数据库连接是否正常

### 2. WebSocket 连接失败

- 检查 Nginx 配置是否正确
- 确认 `Upgrade` 和 `Connection` 头已正确设置
- 检查防火墙设置
- 确认 JWT token 是否有效

### 3. 文件上传失败

- 检查 `uploads/` 目录权限
- 确认 Nginx `client_max_body_size` 配置足够大
- 检查磁盘空间：`df -h`
- 检查文件系统权限

### 4. WebRTC 通话失败

- 检查 SRS 服务是否正常运行：`curl http://localhost/srs/api/v1/versions`
- 确认 UDP 端口（8000）已开放
- 检查 `SRS_CANDIDATE` 配置是否为公网 IP（生产环境）
- 检查防火墙 UDP 端口是否开放
- **重要**：确认客户端通过 `im-server` 的 API 进行 WebRTC 通话，而不是直接连接 SRS
- 检查 `im-server` 是否能正常调用 SRS 的 HTTP API

### 5. JWT 验证失败

- 确认 `im-server` 和 `im-connect` 使用相同的 `JWT_SECRET`
- 检查 token 是否过期
- 确认 token 格式正确

### 6. 数据库连接失败

- 检查 MySQL 服务是否运行：`docker-compose ps mysql`
- 验证数据库连接信息（用户名、密码、数据库名）
- 检查网络连接：`docker network ls`
- 确认数据库已导入：`mysql -h127.0.0.1 -uroot -p123456 -e "USE violet; SHOW TABLES;"`

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

### 贡献指南

1. Fork 本项目
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

## 📄 许可证

[根据项目实际情况填写]

## 📧 联系方式

[根据项目实际情况填写]

---

**紫罗兰IM** - 让每一次沟通都如紫罗兰般优雅而持久 💜
