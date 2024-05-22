use crate::kademlia::kademlia_client::KademliaClient;
use crate::kademlia::{
    FindNodeRequest, FindNodeResponse, FindValueRequest, FindValueResponse, PingRequest,
    PingResponse, StoreRequest, StoreResponse,
};
use crate::kademlia_node_search::config::{TIMEOUT_MAX_ATTEMPTS, TIMEOUT_TIMER};
use crate::kademlia_node_search::node_functions::crypto::Crypto;
use colored::Colorize;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{timeout, Duration};
use tonic::transport::Endpoint;
use tonic::{Request, Status};

pub struct Client;

impl Client {
    async fn attempt_with_timeout<F, Fut, T>(mut attempt: F) -> Result<T, Status>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<T, Status>> + Send,
        T: Send,
    {
        for _ in 0..TIMEOUT_MAX_ATTEMPTS {
            let result = timeout(Duration::from_secs(TIMEOUT_TIMER), attempt()).await;
            match result {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(e)) => eprintln!("{}", format!("Attempt failed: {}", e).red()),
                Err(_) => eprintln!("{}", "Attempt timed out".red()),
            }
        }
        Err(Status::internal("All attempts to send the request failed"))
    }

    pub fn create_ping_request(keypair: &Ed25519KeyPair, self_addr: String) -> PingRequest {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let message = format!("{}{}", self_addr, timestamp).into_bytes();
        let signature = Crypto::sign_message(keypair, &message);
        let sender_public_key = keypair.public_key().as_ref().to_vec();

        PingRequest {
            node_address: self_addr,
            timestamp,
            signature,
            sender_public_key,
        }
    }

    pub async fn send_ping_request(
        request: PingRequest,
        server_addr: String,
    ) -> Result<PingResponse, Status> {
        let endpoint = Endpoint::from_shared(format!("http://{}", server_addr))
            .map_err(|e| Status::internal(format!("Failed to create endpoint: {}", e)))?;

        Client::attempt_with_timeout(|| {
            let endpoint = endpoint.clone();
            let request = request.clone();
            async move {
                let channel = endpoint
                    .connect()
                    .await
                    .map_err(|e| Status::internal(format!("Failed to connect: {}", e)))?;
                let mut client = KademliaClient::new(channel);
                client
                    .ping(request)
                    .await
                    .map(|response| response.into_inner())
            }
        })
        .await
    }

    pub fn create_find_node_request(
        keypair: &Ed25519KeyPair,
        requester_node_id: Vec<u8>,
        requester_node_address: String,
        target_node_id: Vec<u8>,
    ) -> FindNodeRequest {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let message = format!(
            "{:?}{}{:?}{}",
            requester_node_id, requester_node_address, target_node_id, timestamp
        )
        .into_bytes();
        let signature = Crypto::sign_message(keypair, &message);
        let sender_public_key = keypair.public_key().as_ref().to_vec();

        FindNodeRequest {
            requester_node_id,
            requester_node_address,
            target_node_id,
            timestamp,
            signature,
            sender_public_key,
        }
    }

    pub async fn send_find_node_request(
        request: FindNodeRequest,
        target_address: String,
    ) -> Result<FindNodeResponse, Status> {
        let endpoint = Endpoint::from_shared(format!("http://{}", target_address))
            .map_err(|e| Status::internal(format!("Failed to create endpoint: {}", e)))?;

        Client::attempt_with_timeout(|| {
            let endpoint = endpoint.clone();
            let request = request.clone();
            async move {
                let channel = endpoint
                    .connect()
                    .await
                    .map_err(|e| Status::internal(format!("Failed to connect: {}", e)))?;
                let mut client = KademliaClient::new(channel);
                client
                    .find_node(request)
                    .await
                    .map(|response| response.into_inner())
            }
        })
        .await
    }

    pub fn create_store_node_request(
        keypair: &Ed25519KeyPair,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> StoreRequest {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let message = format!("{:?}{:?}{}", key, value, timestamp).into_bytes();
        let signature = Crypto::sign_message(keypair, &message);
        let sender_public_key = keypair.public_key().as_ref().to_vec();

        StoreRequest {
            key,
            value,
            timestamp,
            signature,
            sender_public_key,
        }
    }

    pub async fn send_store_request(
        request: StoreRequest,
        target_address: String,
    ) -> Result<StoreResponse, Status> {
        let endpoint = Endpoint::from_shared(format!("http://{}", target_address))
            .map_err(|e| Status::internal(format!("Failed to create endpoint: {}", e)))?;

        Client::attempt_with_timeout(|| {
            let endpoint = endpoint.clone();
            let request = request.clone();
            async move {
                let channel = endpoint
                    .connect()
                    .await
                    .map_err(|e| Status::internal(format!("Failed to connect: {}", e)))?;
                let mut client = KademliaClient::new(channel);
                client
                    .store(request)
                    .await
                    .map(|response| response.into_inner())
            }
        })
        .await
    }

    pub fn create_find_value_request(keypair: &Ed25519KeyPair, key: Vec<u8>) -> FindValueRequest {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let message = format!("{:?}{}", key, timestamp).into_bytes();
        let signature = Crypto::sign_message(keypair, &message);
        let sender_public_key = keypair.public_key().as_ref().to_vec();

        FindValueRequest {
            key,
            timestamp,
            signature,
            sender_public_key,
        }
    }

    pub async fn send_find_value_request(
        request: FindValueRequest,
        target_address: String,
    ) -> Result<FindValueResponse, Status> {
        let endpoint = Endpoint::from_shared(format!("http://{}", target_address))
            .map_err(|e| Status::internal(format!("Failed to create endpoint: {}", e)))?;

        Client::attempt_with_timeout(|| {
            let endpoint = endpoint.clone();
            let request = request.clone();
            async move {
                let channel = endpoint
                    .connect()
                    .await
                    .map_err(|e| Status::internal(format!("Failed to connect: {}", e)))?;
                let mut client = KademliaClient::new(channel);
                client
                    .find_value(request)
                    .await
                    .map(|response| response.into_inner())
            }
        })
        .await
    }
}