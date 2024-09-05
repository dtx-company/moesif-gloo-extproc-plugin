mod config;
mod event;
mod grpc_service;
mod root_context;
mod utils;

use crate::config::{Config, EnvConfig};
use crate::grpc_service::MoesifGlooExtProcGrpcService;
use envoy_ext_proc_proto::envoy::service::ext_proc::v3::external_processor_server::ExternalProcessorServer as ProcessorServer;
use tonic::transport::Server;

async fn async_main(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:50051".parse()?;

    // Initialize MoesifGlooExtProcGrpcService using the passed config
    let grpc_service = MoesifGlooExtProcGrpcService::new(config).map_err(|e| {
        log::error!("Failed to create gRPC service: {}", e);
        e
    })?;

    println!(
        "Starting Moesif ExtProc gRPC server for Solo.io Gloo Gateway on {}",
        addr
    );

    Server::builder()
        .add_service(ProcessorServer::new(grpc_service))
        .serve(addr)
        .await?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Initialize configuration
    let env_config = EnvConfig::new();
    let config = Config {
        env: env_config,
    };

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async_main(config))
}
