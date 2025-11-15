use std::sync::Arc;
use im_share::{ImMqtt, MqttConfig};

#[derive(Clone)]
pub struct MqttPublisher(Arc<ImMqtt>);

impl MqttPublisher {
    pub fn new(host: &str, port: u16, client_id: &str) -> Self {
        let im = ImMqtt::connect(MqttConfig::new(host, port, client_id));
        Self(Arc::new(im))
    }

    pub async fn publish(&self, topic: &str, payload: Vec<u8>) -> anyhow::Result<()> {
        self.0.publish(topic, payload).await
    }
}

