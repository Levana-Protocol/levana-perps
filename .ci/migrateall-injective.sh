#!/usr/bin/env bash

set -euxo pipefail

export COSMOS_WALLET="$DEPLOYER"
unset COSMOS_GRPC

for COSMOS_NETWORK in injective-testnet
do
    export COSMOS_NETWORK
    echo "store-code for chain: $COSMOS_NETWORK"
    cargo run --bin perps-deploy testnet store-code
done

for PERPS_FAMILY in injdebug injbeta
do
    export PERPS_FAMILY
    echo "migrate for family: $PERPS_FAMILY"
    cargo run --bin perps-deploy testnet migrate
done
