use std::{sync::Arc, time::Duration};

use common_base::{log::error, tools::now_second};
use metadata_struct::mqtt::{lastwill::LastWillData, topic::MQTTTopic};
use tokio::time::sleep;

use crate::storage::{
    keys::{storage_key_mqtt_last_will_prefix, storage_key_mqtt_topic_cluster_prefix},
    mqtt::{lastwill::MQTTLastWillStorage, topic::MQTTTopicStorage},
    rocksdb::RocksDBEngine,
    StorageDataWrap,
};

pub struct MessageExpire {
    cluster_name: String,
    rocksdb_engine_handler: Arc<RocksDBEngine>,
}

impl MessageExpire {
    pub fn new(cluster_name: String, rocksdb_engine_handler: Arc<RocksDBEngine>) -> Self {
        return MessageExpire {
            cluster_name,
            rocksdb_engine_handler,
        };
    }

    pub async fn retain_message_expire(&self) {
        let search_key = storage_key_mqtt_topic_cluster_prefix(&self.cluster_name);
        let topic_storage = MQTTTopicStorage::new(self.rocksdb_engine_handler.clone());

        let cf = self.rocksdb_engine_handler.cf_mqtt();
        let mut iter = self.rocksdb_engine_handler.db.raw_iterator_cf(cf);
        iter.seek(search_key.clone());
        while iter.valid() {
            let key = iter.key();
            let value = iter.value();

            if key == None || value == None {
                iter.next();
                continue;
            }
            let result_key = match String::from_utf8(key.unwrap().to_vec()) {
                Ok(s) => s,
                Err(_) => {
                    iter.next();
                    continue;
                }
            };

            if !result_key.starts_with(&search_key) {
                break;
            }

            let result_value = value.unwrap().to_vec();
            let data = serde_json::from_slice::<StorageDataWrap>(&result_value).unwrap();
            let mut value = serde_json::from_slice::<MQTTTopic>(data.data.as_slice()).unwrap();

            if !value.retain_message.is_none() {
                let delete = if let Some(expired_at) = value.retain_message_expired_at {
                    now_second() >= (data.create_time + expired_at)
                } else {
                    false
                };
                if delete {
                    value.retain_message = None;
                    value.retain_message_expired_at = None;
                    match topic_storage.save(&self.cluster_name, &value.topic_name, value.encode())
                    {
                        Ok(()) => {}
                        Err(e) => {
                            error(e.to_string());
                        }
                    }
                }
            }
            iter.next();
        }
        sleep(Duration::from_secs(1)).await;
    }

    pub async fn last_will_message_expire(&self) {
        let search_key = storage_key_mqtt_last_will_prefix(&self.cluster_name);
        let lastwill_storage = MQTTLastWillStorage::new(self.rocksdb_engine_handler.clone());

        let cf = self.rocksdb_engine_handler.cf_mqtt();
        let mut iter = self.rocksdb_engine_handler.db.raw_iterator_cf(cf);
        iter.seek(search_key.clone());
        while iter.valid() {
            let key = iter.key();
            let value = iter.value();

            if key == None || value == None {
                iter.next();
                continue;
            }
            let result_key = match String::from_utf8(key.unwrap().to_vec()) {
                Ok(s) => s,
                Err(_) => {
                    iter.next();
                    continue;
                }
            };

            if !result_key.starts_with(&search_key) {
                iter.next();
                break;
            }

            let result_value = value.unwrap().to_vec();
            let data = serde_json::from_slice::<StorageDataWrap>(&result_value).unwrap();
            let value = serde_json::from_slice::<LastWillData>(data.data.as_slice()).unwrap();
            if let Some(properties) = value.last_will_properties {
                let delete = if let Some(expiry_interval) = properties.message_expiry_interval {
                    now_second() >= ((expiry_interval as u64) + data.create_time)
                } else {
                    now_second() >= ((86400 * 30) + data.create_time)
                };

                if delete {
                    match lastwill_storage
                        .delete_last_will_message(&self.cluster_name, &value.client_id)
                    {
                        Ok(()) => {}
                        Err(e) => {
                            error(e.to_string());
                        }
                    }
                }
            }

            iter.next();
        }
        sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::{
        mqtt::{
            lastwill::MQTTLastWillStorage, session::MQTTSessionStorage, topic::MQTTTopicStorage,
        },
        rocksdb::RocksDBEngine,
    };
    use common_base::{
        config::placement_center::PlacementCenterConfig,
        tools::{now_second, unique_id},
    };
    use metadata_struct::mqtt::{
        lastwill::LastWillData, message::MQTTMessage, session::MQTTSession, topic::MQTTTopic,
    };
    use protocol::mqtt::common::{LastWillProperties, Publish};
    use std::{sync::Arc, time::Duration};
    use tokio::time::sleep;

    use super::MessageExpire;

    #[tokio::test]
    async fn retain_message_expire_test() {
        let config = PlacementCenterConfig::default();
        let cluster_name = unique_id();
        let rocksdb_engine_handler = Arc::new(RocksDBEngine::new(&config));
        let message_expire =
            MessageExpire::new(cluster_name.clone(), rocksdb_engine_handler.clone());

        let topic_storage = MQTTTopicStorage::new(rocksdb_engine_handler.clone());
        let topic = MQTTTopic::new(unique_id(), "tp1".to_string());
        topic_storage
            .save(&cluster_name, &topic.topic_name, topic.encode())
            .unwrap();

        let retain_msg = MQTTMessage::build_message(&"c1".to_string(), &Publish::default(), &None);
        topic_storage
            .set_topic_retain_message(&cluster_name, &topic.topic_name, retain_msg.encode(), 3)
            .unwrap();
        tokio::spawn(async move {
            loop {
                message_expire.retain_message_expire().await;
            }
        });

        let start = now_second();
        loop {
            let res = topic_storage
                .list(&cluster_name, Some(topic.topic_name.clone()))
                .unwrap();
            let data = res.get(0).unwrap();
            let tp = serde_json::from_slice::<MQTTTopic>(data.data.as_slice()).unwrap();
            if tp.retain_message.is_none() {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        assert_eq!((now_second() - start), 3);
    }

    #[tokio::test]
    async fn last_will_message_expire_test() {
        let config = PlacementCenterConfig::default();
        let cluster_name = unique_id();
        let rocksdb_engine_handler = Arc::new(RocksDBEngine::new(&config));
        let lastwill_storage = MQTTLastWillStorage::new(rocksdb_engine_handler.clone());
        let session_storage = MQTTSessionStorage::new(rocksdb_engine_handler.clone());

        let client_id = unique_id();
        let mut last_will_properties = LastWillProperties::default();
        last_will_properties.message_expiry_interval = Some(3);
        let last_will_message = LastWillData {
            client_id: client_id.clone(),
            last_will: None,
            last_will_properties: Some(last_will_properties),
        };
        let message_expire =
            MessageExpire::new(cluster_name.clone(), rocksdb_engine_handler.clone());
        tokio::spawn(async move {
            loop {
                message_expire.last_will_message_expire().await;
            }
        });

        let mut session = MQTTSession::default();
        session.client_id = client_id.clone();
        session_storage
            .save(&cluster_name, &client_id, session.encode())
            .unwrap();
        lastwill_storage
            .save(&cluster_name, &client_id, last_will_message.encode())
            .unwrap();

        let start = now_second();
        loop {
            let res = lastwill_storage.get(&cluster_name, &client_id).unwrap();
            if res.is_none() {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }

        assert_eq!((now_second() - start), 3);
    }
}
