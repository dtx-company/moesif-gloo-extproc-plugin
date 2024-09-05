#!/bin/bash -e

# Set default tag to 'latest' unless provided as an argument
TAG=${1:-latest}

# Determine build variant: debug or release
if [ "$2" == "debug" ]; then
    BUILD_VARIANT=debug
    BUILD_FLAGS=""
else
    BUILD_VARIANT=release
    BUILD_FLAGS="--release"
fi

# Docker image names
# REPO=docker.io/moesif
REPO=gcr.io/solo-test-236622

# Get the directory of this script to make sure we can run it from anywhere
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_DIR="$SCRIPT_DIR/../.."
SOURCE="$BASE_DIR/moesif-extproc"
OUTPUT="$SOURCE/target/$BUILD_VARIANT"

# Grab the version from Cargo.toml
VERSION=$(grep -m 1 '^version =' "$SOURCE/Cargo.toml" | awk -F\" '{print $2}')

# Proceed with latest if no version is found
if [ -z "$VERSION" ]; then
    echo "Version not found in Cargo.toml. Proceeding with 'latest' tag only."
    TAG_ARTIFACT=$REPO/moesif-extproc-plugin:latest
else
    TAG_ARTIFACT_VERSION=$REPO/moesif-extproc-plugin:$VERSION
    TAG_ARTIFACT_LATEST=$REPO/moesif-extproc-plugin:latest
fi
# Step 1: Build the Rust binary locally
echo "Building the Rust binary..."
cargo build --manifest-path $SOURCE/Cargo.toml $BUILD_FLAGS

# Step 2: Ensure the output binary exists
if [ ! -f "$OUTPUT/moesif_envoy_extproc_plugin" ]; then
    echo "Build failed or binary not found!"
    exit 1
fi

# Step 3: Build the Docker image
if [ -z "$VERSION" ]; then
    echo "Building the Docker image with tag: latest..."
    docker build \
    -t $TAG_ARTIFACT \
    -f $SOURCE/Dockerfile \
    --build-arg BINARY_PATH=$OUTPUT/moesif_envoy_extproc_plugin \
    $SOURCE
else
    echo "Building the Docker image with tags: $VERSION and latest..."
    docker build \
    -t $TAG_ARTIFACT_VERSION \
    -t $TAG_ARTIFACT_LATEST \
    -f $SOURCE/Dockerfile \
    --build-arg BINARY_PATH=$OUTPUT/moesif_envoy_extproc_plugin \
    $SOURCE
fi

# Uncomment the following lines to push the Docker image to the repository
# docker push $TAG_ARTIFACT_VERSION
# docker push $TAG_ARTIFACT_LATESTdoc

echo "Docker image built successfully."
