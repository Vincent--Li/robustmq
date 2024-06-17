use crate::{
    cache::{cluster::ClusterCache, mqtt::MqttCache},
    core::share_sub::calc_share_sub_leader,
    raft::apply::{RaftMachineApply, StorageData, StorageDataType},
    storage::{
        mqtt::{session::MQTTSessionStorage, topic::MQTTTopicStorage, user::MQTTUserStorage},
        rocksdb::RocksDBEngine,
    },
};
use prost::Message;
use protocol::placement_center::generate::{
    common::CommonReply,
    mqtt::{
        mqtt_service_server::MqttService, CreateSessionRequest, CreateTopicRequest,
        CreateUserRequest, DeleteSessionRequest, DeleteTopicRequest, DeleteUserRequest,
        GetShareSubLeaderReply, GetShareSubLeaderRequest, ListSessionReply, ListSessionRequest,
        ListTopicReply, ListTopicRequest, ListUserReply, ListUserRequest,
    },
};
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct GrpcMqttService {
    cluster_cache: Arc<ClusterCache>,
    mqtt_cache: Arc<MqttCache>,
    placement_center_storage: Arc<RaftMachineApply>,
    rocksdb_engine_handler: Arc<RocksDBEngine>,
}

impl GrpcMqttService {
    pub fn new(
        cluster_cache: Arc<ClusterCache>,
        mqtt_cache: Arc<MqttCache>,
        placement_center_storage: Arc<RaftMachineApply>,
        rocksdb_engine_handler: Arc<RocksDBEngine>,
    ) -> Self {
        GrpcMqttService {
            cluster_cache,
            mqtt_cache,
            placement_center_storage,
            rocksdb_engine_handler,
        }
    }
}

impl GrpcMqttService {}

#[tonic::async_trait]
impl MqttService for GrpcMqttService {
    async fn get_share_sub_leader(
        &self,
        request: Request<GetShareSubLeaderRequest>,
    ) -> Result<Response<GetShareSubLeaderReply>, Status> {
        let req = request.into_inner();
        let cluster_name = req.cluster_name;
        let group_name = req.group_name;
        let mut reply = GetShareSubLeaderReply::default();

        let leader_broker = match calc_share_sub_leader(
            cluster_name.clone(),
            group_name.clone(),
            self.cluster_cache.clone(),
        ) {
            Ok(data) => data,
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        };
        if let Some(node) = self.cluster_cache.get_node(cluster_name, leader_broker) {
            reply.broker_id = leader_broker;
            reply.broker_addr = node.node_inner_addr;
            reply.extend_info = node.extend;
        }
        return Ok(Response::new(reply));
    }

    async fn list_user(
        &self,
        request: Request<ListUserRequest>,
    ) -> Result<Response<ListUserReply>, Status> {
        let req = request.into_inner();
        let storage = MQTTUserStorage::new(self.rocksdb_engine_handler.clone());
        match storage.list(req.cluster_name, Some(req.username)) {
            Ok(data) => {
                let mut result = Vec::new();
                for raw in data {
                    result.push(raw.data);
                }
                let reply = ListUserReply { users: result };

                return Ok(Response::new(reply));
            }
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        }
    }

    async fn create_user(
        &self,
        request: Request<CreateUserRequest>,
    ) -> Result<Response<CommonReply>, Status> {
        let req = request.into_inner();

        let data = StorageData::new(
            StorageDataType::MQTTCreateUser,
            CreateUserRequest::encode_to_vec(&req),
        );

        match self
            .placement_center_storage
            .apply_propose_message(data, "create_user".to_string())
            .await
        {
            Ok(_) => return Ok(Response::new(CommonReply::default())),
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        }
    }

    async fn delete_user(
        &self,
        request: Request<DeleteUserRequest>,
    ) -> Result<Response<CommonReply>, Status> {
        let req = request.into_inner();

        let data = StorageData::new(
            StorageDataType::MQTTDeleteUser,
            DeleteUserRequest::encode_to_vec(&req),
        );

        match self
            .placement_center_storage
            .apply_propose_message(data, "delete_user".to_string())
            .await
        {
            Ok(_) => return Ok(Response::new(CommonReply::default())),
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        }
    }

    async fn create_topic(
        &self,
        request: Request<CreateTopicRequest>,
    ) -> Result<Response<CommonReply>, Status> {
        let req = request.into_inner();
        let data = StorageData::new(
            StorageDataType::MQTTCreateTopic,
            CreateTopicRequest::encode_to_vec(&req),
        );

        match self
            .placement_center_storage
            .apply_propose_message(data, "create_topic".to_string())
            .await
        {
            Ok(_) => return Ok(Response::new(CommonReply::default())),
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        }
    }

    async fn delete_topic(
        &self,
        request: Request<DeleteTopicRequest>,
    ) -> Result<Response<CommonReply>, Status> {
        let req = request.into_inner();
        let data = StorageData::new(
            StorageDataType::MQTTDeleteTopic,
            DeleteTopicRequest::encode_to_vec(&req),
        );

        match self
            .placement_center_storage
            .apply_propose_message(data, "delete_topic".to_string())
            .await
        {
            Ok(_) => return Ok(Response::new(CommonReply::default())),
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        }
    }

    async fn list_topic(
        &self,
        request: Request<ListTopicRequest>,
    ) -> Result<Response<ListTopicReply>, Status> {
        let req = request.into_inner();
        let storage = MQTTTopicStorage::new(self.rocksdb_engine_handler.clone());
        match storage.list(req.cluster_name, Some(req.topic_name)) {
            Ok(data) => {
                let mut result = Vec::new();
                for raw in data {
                    result.push(raw.data);
                }
                let reply = ListTopicReply { topics: result };

                return Ok(Response::new(reply));
            }
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        }
    }

    async fn list_session(
        &self,
        request: Request<ListSessionRequest>,
    ) -> Result<Response<ListSessionReply>, Status> {
        let req = request.into_inner();
        let storage = MQTTSessionStorage::new(self.rocksdb_engine_handler.clone());
        match storage.list(req.cluster_name, Some(req.client_id)) {
            Ok(data) => {
                let mut result = Vec::new();
                for raw in data {
                    result.push(raw.data);
                }
                let reply = ListSessionReply { sessions: result };
                return Ok(Response::new(reply));
            }
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        }
    }

    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<CommonReply>, Status> {
        let req = request.into_inner();
        let data = StorageData::new(
            StorageDataType::MQTTCreateSession,
            CreateSessionRequest::encode_to_vec(&req),
        );

        match self
            .placement_center_storage
            .apply_propose_message(data, "create_session".to_string())
            .await
        {
            Ok(_) => return Ok(Response::new(CommonReply::default())),
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        }
    }

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<CommonReply>, Status> {
        let req = request.into_inner();
        let data = StorageData::new(
            StorageDataType::MQTTDeleteSession,
            DeleteSessionRequest::encode_to_vec(&req),
        );

        match self
            .placement_center_storage
            .apply_propose_message(data, "delete_session".to_string())
            .await
        {
            Ok(_) => return Ok(Response::new(CommonReply::default())),
            Err(e) => {
                return Err(Status::cancelled(e.to_string()));
            }
        }
    }
}
