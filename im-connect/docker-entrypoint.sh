#!/bin/bash
set -e

# 从环境变量生成配置文件
cat > /app/config.toml <<EOF
[mqtt]
host = "${MQTT_HOST:-mqtt}"
port = ${MQTT_PORT:-1883}

[connect]
port = ${CONNECT_PORT:-3001}

[redis]
host = "${REDIS_HOST:-redis}"
port = ${REDIS_PORT:-6379}
db = ${REDIS_DB:-0}

[jwt]
secret = "${JWT_SECRET:-337eb69ef604dec5cdb04481242877fea7db31e4c1fd236497033431ab41d499}"
expiration_hours = ${JWT_EXPIRATION_HOURS:-24}
EOF

# 如果设置了 Redis 密码，添加到配置中
if [ -n "$REDIS_PASSWORD" ]; then
    echo 'password = "'"$REDIS_PASSWORD"'"' >> /app/config.toml
fi

# 执行主程序
exec "$@"

