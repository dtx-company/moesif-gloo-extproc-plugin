FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY target/release/moesif_envoy_extproc_plugin /usr/local/bin/moesif_envoy_extproc_plugin

CMD ["moesif_envoy_extproc_plugin"]
