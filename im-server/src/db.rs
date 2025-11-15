use sqlx::MySqlPool;
use crate::config::AppConfig;
use tracing::info;

pub async fn create_pool(cfg: &AppConfig) -> anyhow::Result<MySqlPool> {
    let db_cfg = &cfg.database;
    
    let database_url = format!(
        "mysql://{}:{}@{}:{}/{}",
        db_cfg.user, db_cfg.password, db_cfg.host, db_cfg.port, db_cfg.database
    );
    
    info!("连接数据库: {}@{}:{}/{}", db_cfg.user, db_cfg.host, db_cfg.port, db_cfg.database);
    
    let pool = MySqlPool::connect(&database_url).await?;
    
    info!("数据库连接池创建成功");
    
    Ok(pool)
}

