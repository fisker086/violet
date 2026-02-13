# im-connect (Rust)

基于 Lucky-cloud 的 im-connect 重写：WebSocket 长连接网关，技术栈为 Rust。

## 技术栈

- **协议**：WebSocket 二进制帧，消息体为 Protobuf（`im_message_wrap.proto`）
- **鉴权**：JWT（支持 Query / `Authorization: Bearer` / Cookie）
- **用户会话**：Redis（`IM-USER-{userId}` 存路由信息）
- **消息队列**：RabbitMQ（按 `brokerId` 队列消费，按 `code` 分发到连接）
- **服务发现**：Nacos（可选，注册 WebSocket 端口）

## 配置

见 `config.toml`，支持环境变量覆盖：`IM__NETTY__WEBSOCKET__PORTS=19000,19001` 等。

- `broker_id`：当前节点 ID，兼作 RabbitMQ 队列名，需与业务路由一致
- `netty.websocket.ports`：监听端口列表
- `nacos.register_enabled`：是否注册到 Nacos

## 构建与运行

```bash
# 构建
cargo build --release

# 使用默认配置启动
./target/release/im-connect

# 指定配置文件
./target/release/im-connect -c config.toml

# 指定日志级别
./target/release/im-connect --log-level debug

# 组合使用
./target/release/im-connect -c config.toml --log-level info

# 显示版本信息
./target/release/im-connect --version

# 显示帮助信息
./target/release/im-connect --help
```

### 命令行参数

- `-c, --config <FILE>`: 指定配置文件路径（默认: `config.toml`）
- `-l, --log-level <LEVEL>`: 日志级别，可选值: `trace`, `debug`, `info`, `warn`, `error`（默认: `info`）
- `-V, --version`: 显示版本信息并退出
- `-h, --help`: 显示帮助信息

**注意**: 环境变量 `RUST_LOG` 会覆盖 `--log-level` 参数。

## 流程简述

1. **握手**：HTTP 升级时从 Query/Header/Cookie 取 token，JWT 校验得到 `userId`，再升级为 WebSocket。
2. **登录**：首包必须为 REGISTER（code=200）的 Proto 二进制；服务端写 Redis、加入 `UserChannelMap`，并回 REGISTER_SUCCESS。
3. **下行**：RabbitMQ 消费到 JSON 消息，按 `code` 与 `ids` 找到对应用户连接，转成 Proto 二进制推送到 WebSocket。
4. **心跳**：客户端发 HEART_BEAT(206)，服务端回 HEART_BEAT_SUCCESS(207)。

## Proto

- 定义：`proto/im_message_wrap.proto`（与 Lucky-cloud 一致，含 `google.protobuf.Any data`）
- 构建时由 `prost-build` 生成 Rust 代码到 `OUT_DIR`，见 `src/proto/mod.rs`。

## 与 Java 版对应关系

| Java (Lucky-cloud)        | Rust (本仓库)     |
|---------------------------|-------------------|
| WebSocketTemplate         | `ws::ws_handler` + axum |
| AuthHandler               | `auth::*` + 握手时校验 |
| LoginProcess              | `login::do_register`   |
| UserChannelMap            | `channel::UserChannelMap` |
| RabbitMQService          | `mq::run_consumer`    |
| NacosRegistrationService  | `nacos::register_websocket_ports` |
| RedisService / 用户路由   | `redis::*` + Login 时写 Redis |
