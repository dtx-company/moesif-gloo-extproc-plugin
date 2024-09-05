fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut proto_config = prost_build::Config::new();
    proto_config.bytes(["."]);
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_with_config(
            proto_config,
            &["data-plane-api/envoy/service/ext_proc/v3/external_processor.proto"],
            &["data-plane-api/", "udpa/", "protoc-gen-validate/", "xds/"],
        )?;
    Ok(())
}
