use crate::kademlia::kademlia_server::{Kademlia, KademliaServer};
use crate::kademlia::{
    FindNodeRequest, FindNodeResponse, FindValueRequest, FindValueResponse, PingRequest,
    PingResponse, StoreRequest, StoreResponse,
};

use super::node_functions::client::Client;
use super::node_functions::crypto::Crypto;
use super::node_functions::request_handler::RequestHandler;
use super::node_functions::routing_table::NodeInfo;
use super::node_functions::routing_table::RoutingTable;

use bytes::Bytes;
use colored::*;
use rand_chacha::rand_core::SeedableRng;
use rand_distr::{Distribution, Uniform};
use ring::digest::{digest, SHA256};
use ring::signature;
use ring::signature::KeyPair;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
//Config Constants
use super::config::{C1, LOG_INTERVAL, REFRESH_TIMER_LOWER, REFRESH_TIMER_UPPER};
#[derive(Debug)]
pub struct Node {
    pub keypair: signature::Ed25519KeyPair,
    pub id: Bytes,
    pub addr: SocketAddr,
    pub storage: Mutex<HashMap<Bytes, Bytes>>,
    pub routing_table: Mutex<RoutingTable>,
}

impl Node {
    pub async fn new(
        addr: SocketAddr,
        bootstrap_addr: Option<String>,
    ) -> Result<Arc<Mutex<Self>>, Box<dyn std::error::Error>> {
        let (keypair, node_id, duration, attempts) = Self::generate_id().await?;
        let routing_table = Mutex::new(RoutingTable::new(node_id.clone()));

        // Create a node instance within an Arc<Mutex<>> wrapper
        let node = Arc::new(Mutex::new(Node {
            keypair,
            id: node_id,
            addr,
            storage: Mutex::new(HashMap::new()),
            routing_table,
        }));

        // Print out the generated node ID
        println!("Generated Node ID: {}", hex::encode(&node.lock().await.id));
        println!("Time taken to generate Node ID: {:.2?}", duration);
        println!("Number of attempts: {}", attempts);

        // Fetch the bootstrap node's routing table if provided
        if let Some(addr) = bootstrap_addr {
            println!(
                "{}",
                format!("Fetching routing table from bootstrap node: {}", addr).blue()
            );
            node.lock().await.fetch_routing_table(&addr).await?;
        }

        node.lock().await.routing_table.lock().await.print_table();

        Ok(node)
    }

    async fn generate_id(
    ) -> Result<(signature::Ed25519KeyPair, Bytes, Duration, u64), Box<dyn std::error::Error>> {
        let c1 = C1; // Example difficulty level: number of leading zero bits
        let start_time = Instant::now();
        let mut attempts = 0;
        let attempt_log_interval = LOG_INTERVAL; // Print status every 10,000 attempts

        println!("Generating node ID with {} leading zero bits", c1);

        loop {
            attempts += 1;
            let keypair = Crypto::create_keypair()?;
            let public_key_hash = digest(&SHA256, keypair.public_key().as_ref());

            let node_id = Bytes::from(public_key_hash.as_ref().to_vec());

            if attempts % attempt_log_interval == 0 {
                let elapsed = start_time.elapsed();
                println!(
                    "Attempts: {}, Elapsed time: {:.2?} seconds",
                    attempts, elapsed
                );
            }

            // Check if the first `c1` bits are zero (this implementation only works if c1 < 16)
            let valid = if c1 <= 8 {
                node_id[0] >> (8 - c1) == 0
            } else {
                node_id[0] == 0 && node_id[1] >> (16 - c1) == 0
            };

            if valid {
                let duration = start_time.elapsed();
                return Ok((keypair, node_id, duration, attempts));
            }
        }
    }

    async fn fetch_routing_table(
        &self,
        target_addr: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ping_request = Client::create_ping_request(&self.keypair, self.addr.to_string());
        let ping_response =
            Client::send_ping_request(ping_request, target_addr.to_string()).await?;

        println!(
            "{}",
            format!("Received ping response: {:?}", ping_response).green()
        );

        let find_node_request = Client::create_find_node_request(
            &self.keypair,
            self.id.to_vec(),
            self.addr.to_string(),
            ping_response.node_id.to_vec(),
        );

        let find_node_response =
            Client::send_find_node_request(find_node_request, target_addr.to_string()).await?;

        println!(
            "{}",
            format!("Received find_node response: {:?}", find_node_response).green()
        );

        self.update_routing_table(RoutingTable::from_proto_nodes(find_node_response.nodes))
            .await;

        Ok(())
    }

    async fn update_routing_table(&self, nodes: Vec<NodeInfo>) {
        let mut routing_table = self.routing_table.lock().await;
        let mut _counter: i64 = 0;
        for new_node in nodes {
            if self.id != new_node.id && !routing_table.contains(&new_node.id) {
                _counter += 1;
                routing_table.add_node(new_node, &self.id);
            }
        }

        println!("Added {} new nodes to the routing table", _counter);
    }

    async fn refresh_routing_table(node: Arc<Mutex<Node>>) {
        let interval_range = Uniform::from(REFRESH_TIMER_LOWER..REFRESH_TIMER_UPPER);
        let mut rng = rand_chacha::ChaChaRng::from_entropy(); // RNG should be outside the loop to preserve state and performance

        loop {
            let sleep_time = interval_range.sample(&mut rng);
            println!(
                "{}",
                format!("Fetching routing table in {} seconds", sleep_time).cyan()
            );
            tokio::time::sleep(Duration::from_secs(sleep_time)).await;

            // Lock only when needed and scope the lock to minimize blocking
            let maybe_node_info = {
                let node_lock = node.lock().await;
                let routing_table = node_lock.routing_table.lock().await;
                routing_table.random_node().cloned() // Clone the data to use outside the lock
            };

            if let Some(node_info) = maybe_node_info {
                println!(
                    "{}",
                    format!("Refreshing routing table from node: {:?}", node_info.addr).cyan()
                );
                // Fetch the routing table outside of the node locks
                let result = {
                    let node_lock = node.lock().await;
                    node_lock
                        .fetch_routing_table(&node_info.addr.to_string())
                        .await
                };

                match result {
                    Ok(_) => println!(
                        "{}",
                        format!(
                            "Routing table refreshed successfully from {}",
                            node_info.addr
                        )
                        .green()
                    ),
                    Err(e) => eprintln!(
                        "{}",
                        format!(
                            "Failed to refresh routing table from {}: {}",
                            node_info.addr, e
                        )
                        .red()
                    ),
                }
            } else {
                eprintln!(
                    "{}",
                    "No nodes available to refresh the routing table".yellow()
                );
            }
            node.lock().await.routing_table.lock().await.print_table();
        }
    }
}

#[tonic::async_trait]
impl Kademlia for Arc<Mutex<Node>> {
    async fn ping(&self, request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        let node = Arc::clone(self);
        println!("{}", format!("Received ping request: {:?}", request).blue());

        RequestHandler::handle_ping(node, request).await
    }

    async fn store(
        &self,
        request: Request<StoreRequest>,
    ) -> Result<Response<StoreResponse>, Status> {
        let node = Arc::clone(self);
        println!(
            "{}",
            format!("Received store request: {:?}", request).blue()
        );

        RequestHandler::handle_store(node, request).await
    }

    async fn find_node(
        &self,
        request: Request<FindNodeRequest>,
    ) -> Result<Response<FindNodeResponse>, Status> {
        let node = Arc::clone(self);
        println!(
            "{}",
            format!("Received find_node request: {:?}", request).blue()
        );

        RequestHandler::handle_find_node(node, request).await
    }

    async fn find_value(
        &self,
        request: Request<FindValueRequest>,
    ) -> Result<Response<FindValueResponse>, Status> {
        let node = Arc::clone(self);
        println!(
            "{}",
            format!("Received find_value request: {:?}", request).blue()
        );

        RequestHandler::handle_find_value(node, request).await
    }
}

pub async fn run_server(addr: SocketAddr, node: Arc<Mutex<Node>>) {
    tokio::spawn(Node::refresh_routing_table(node.clone()));

    println!("{}", format!("Server listening on {}", addr).green());
    Server::builder()
        .add_service(KademliaServer::new(Arc::clone(&node.clone())))
        .serve(addr)
        .await
        .unwrap();
}