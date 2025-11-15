use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 简单的雪花ID生成器实现
struct SnowflakeGenerator {
    machine_id: u64,
    datacenter_id: u64,
    sequence: u64,
    last_timestamp: u64,
}

impl SnowflakeGenerator {
    fn new(machine_id: u64, datacenter_id: u64) -> Self {
        Self {
            machine_id,
            datacenter_id,
            sequence: 0,
            last_timestamp: 0,
        }
    }

    fn next_id(&mut self) -> u64 {
        let mut timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        if timestamp < self.last_timestamp {
            // 时钟回拨，等待
            timestamp = self.last_timestamp;
        }

        if timestamp == self.last_timestamp {
            self.sequence = (self.sequence + 1) & 0xFFF; // 12位序列号
            if self.sequence == 0 {
                // 序列号溢出，等待下一毫秒
                timestamp = self.wait_next_millis(self.last_timestamp);
            }
        } else {
            self.sequence = 0;
        }

        self.last_timestamp = timestamp;

        // 雪花ID结构：1位符号位 + 41位时间戳 + 10位机器ID(5位datacenter + 5位machine) + 12位序列号
        (timestamp << 22) | ((self.datacenter_id & 0x1F) << 17) | ((self.machine_id & 0x1F) << 12) | (self.sequence & 0xFFF)
    }

    fn wait_next_millis(&self, last_timestamp: u64) -> u64 {
        let mut timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        while timestamp <= last_timestamp {
            timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
        }
        timestamp
    }
}

// 雪花ID生成器（单例模式）
lazy_static::lazy_static! {
    static ref SNOWFLAKE_GENERATOR: Mutex<SnowflakeGenerator> = {
        Mutex::new(SnowflakeGenerator::new(1, 1))
    };
}

/// 生成雪花ID
pub fn generate_snowflake_id() -> u64 {
    let mut generator = SNOWFLAKE_GENERATOR.lock().unwrap();
    generator.next_id()
}

/// 使用指定的机器ID和数据中心ID生成雪花ID
pub fn generate_snowflake_id_with_config(machine_id: u64, datacenter_id: u64) -> u64 {
    let mut generator = SnowflakeGenerator::new(machine_id, datacenter_id);
    generator.next_id()
}

