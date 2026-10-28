use std::sync::Arc;

use solana_client::nonblocking::rpc_client::RpcClient;

pub type RpcInstance = Arc<RpcClient>;
