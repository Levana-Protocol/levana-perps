set dotenv-load
export PERPS_WASM_DIR := "./wasm/artifacts"
GIT_SHA := `git rev-parse HEAD`

# List all recipies
default:
	just --list --unsorted

# Build localosmosis
build-localosmosis:
	cd ./.ci && docker image build . -f osmolocal.Dockerfile -t localosmo

# Run localosmo
run-localosmo:
	./.ci/osmolocal.sh

# Stop localosmo
stop-localosmo:
	docker stop osmolocaltest

# cargo compile
cargo-compile:
	cargo test --workspace --no-run --locked

# Market tests
market-test collateral-type token-kind:
	env MARKET_COLLATERAL_TYPE={{collateral-type}} MARKET_TOKEN_KIND={{token-kind}} cargo test --workspace --locked

# cargo tests check
cargo-test-check:
	just market-test base native
	# Commenting these tests to save time
	# just market-test quote native
	# just market-test base cw20
	just market-test quote cw20

# Property tests
cargo-test-with-prop:
	just prop-test-run base native
	# Commenting these tests to save time
	# just market-test quote native
	# just market-test base cw20
	just prop-test-run quote cw20

prop-test-run collateral-type token-kind:
	env MARKET_COLLATERAL_TYPE={{collateral-type}} MARKET_TOKEN_KIND={{token-kind}} cargo test --workspace --locked --features proptest

# cargo clippy check
cargo-clippy-check:
    cargo clippy --no-deps --workspace --locked --tests --benches --examples -- -Dwarnings

# cargo fmt check
cargo-fmt-check:
	cargo fmt --all --check

# Run tests, clippy, and format
cargo-full-check:
	just cargo-test-check
	just cargo-clippy-check
	just cargo-fmt-check

# Build contracts with Cosmos Docker tooling
build-contracts:
	./.ci/contracts.sh

# Build contracts with native tooling
build-contracts-native:
	./.ci/native-contract-build.sh

# Deploy contracts to LocalOsmosis
local-deploy:
	COSMOS_WALLET=osmosis-local cargo run --bin perps-deploy local-deploy --network osmosis-local

# Deploy contracts to a local instance of wasmd (see levana-contracts-rs for expected wasmd config)
local-deploy-wasmd:
	cargo run --bin perps-deploy local-deploy --network wasmd-local

# Run on chain tests
contracts-test:
	COSMOS_WALLET=osmosis-local cargo run --bin perps-deploy on-chain-tests --network osmosis-local

# Run on chain tests without running local osmosis
contracts-test-skip-osmosis:
	COSMOS_WALLET=osmosis-local cargo run --bin perps-deploy on-chain-tests --skip-init --network osmosis-local

# Run on chain tests against wasmd (without spinning it up, see levana-contracts-rs for expected wasmd config)
contracts-test-wasmd:
	cargo run --bin perps-deploy on-chain-tests --skip-init --network wasmd-local

# Cache docker images by saving it under wasm
cache-docker-images:
	mkdir -p wasm/images
	-docker load -i ./wasm/images/workspace_0.12.10.tar
	-[ -f wasm/images/workspace_0.12.10.tar ] || docker pull cosmwasm/workspace-optimizer:0.12.10 && docker save cosmwasm/workspace-optimizer:0.12.10 > wasm/images/workspace_0.12.10.tar

# Typescript check for CI which needs deps installed
typescript-check:
	cd ts-schema && yarn install
	just typescript-schema

# Typescript schema
typescript-schema:
	cd packages/msg && cargo run --example generate-schema
	cd ts-schema && yarn && yarn build

# Generate the schema and copy to a webapp directory located at ../webapp
typescript-schema-copy: typescript-schema
	rm -rf ../webapp/src/apps/perps/sdk/types/.generated/
	mv schema/typescript ../webapp/src/apps/perps/sdk/types/.generated/

# Build perps-qa binary in release mode
cargo-release:
    cargo build --bin perps-qa --release --bins --locked

# Build bots binary in release mode
cargo-bots-release:
    cargo build --bin perps-bots --release --target x86_64-unknown-linux-musl

# Build bots docker image
build-bots-image:
	cp target/x86_64-unknown-linux-musl/release/perps-bots .ci/bots
	cd .ci/bots && docker image build . -f Dockerfile -t ghcr.io/levana-protocol/levana-perps/bots:{{GIT_SHA}}

# Push bots docker image
push-bots-image:
	docker push ghcr.io/levana-protocol/levana-perps/bots:{{GIT_SHA}}

# Deploy to dragonfire
deploy-dragonfire:
	cargo run --bin perps-deploy store-code --network dragonfire

# Migrate dragonci
migrate-dragonci:
	cargo run --bin perps-deploy migrate --family dragonci

# Build documentations
build-docs:
	mkdir -p ./.output/temp/schema/cosmos
	cargo doc --no-deps --package levana_perpswap_cosmos_msg --target-dir=./.output/temp/api/cosmos/msg
	cargo doc --no-deps --package levana_perpswap_cosmos_shared --target-dir=./.output/temp/api/cosmos/shared
	echo "<html><body><h1>perpetual swaps</h1></body></html>" > ./.output/temp/index.html

# Coverage with specific collateral and market token kind
coverage-test collateral-type token-kind:
	env MARKET_COLLATERAL_TYPE={{collateral-type}} MARKET_TOKEN_KIND={{token-kind}} cargo llvm-cov --no-report --locked --package levana_perpswap_multi_test

# Off chain Coverage report
off-chain-coverage:
	cargo llvm-cov clean --workspace
	just coverage-test base native
	just coverage-test quote native
	just coverage-test base cw20
	just coverage-test quote cw20

# Off chain coverage with html report
off-chain-html-coverage:
	cargo llvm-cov report --html --open

# Off chain coverage with terminal summary
off-chain-term-coverage:
	cargo llvm-cov report --summary-only

# Run the fuzz tester. Get a cup of coffee.
fuzz:
	cargo +nightly fuzz run market --fuzz-dir packages/fuzz

# For right now, justfiles to not support parallel execution
# so use npm-run-all to kick things off
# build-type: release or dev
# exec-type: sanity or performance
diagnostics-gui build-type exec-type:
	cd ./packages/diagnostics && yarn serve:{{build-type}}:{{exec-type}}

# Run bots directly (for dev purposes, not for production)
bots:
	cargo run --bin perps-bots

# Rewards
store-rewards:
	just store-hatching
	just store-ibc-execute
	just store-lvn-rewards
instantiate-rewards:
	just instantiate-hatching
	just instantiate-nft-mint
	just instantiate-lvn-rewards
create-rewards-channels juno-port stargaze-port osmosis-port:
	just create-nft-mint-relayer-channel hatching-nft {{juno-port}} {{stargaze-port}} 
	just create-lvn-grant-relayer-channel lvn-mint {{juno-port}} {{osmosis-port}}
rewards-test:
	just hatch-egg-test
rewards-relayer-start:
	rly start hatching-nft --debug
	# TODO - add lvn
	# rly start lvn-mint --debug

# Rewards subcommands
store-hatching:
	cargo run --bin perps-deploy store-code --contracts=hatching --network=juno-testnet
store-lvn-rewards:
	cargo run --bin perps-deploy store-code --contracts=lvn-rewards --network=osmosis-testnet
instantiate-hatching:
	cargo run --bin perps-deploy instantiate-rewards --contracts=hatching --network=juno-testnet
instantiate-lvn-rewards:
	cargo run --bin perps-deploy instantiate-rewards --contracts=lvn-rewards --network=osmosis-testnet
store-ibc-execute:
	cargo run --bin perps-deploy store-code --contracts=ibc-execute-proxy --network=stargaze-testnet
instantiate-nft-mint:
	cargo run --bin perps-deploy instantiate-rewards --contracts=ibc-execute-proxy --ibc-execute-proxy-target=nft-mint --network=stargaze-testnet
hatch-egg-test:
	cargo run --bin rewards-test hatch-egg --hatch-network=juno-testnet --nft-mint-network=stargaze-testnet --lvn-rewards-network=osmosis-testnet
create-nft-mint-relayer-channel path-name juno-port stargaze-port:
	rly transact channel {{path-name}} --src-port {{juno-port}} --dst-port {{stargaze-port}} --order unordered --version nft-mint-001 --debug --override
create-lvn-grant-relayer-channel path-name juno-port osmosis-port:
	rly transact channel {{path-name}} --src-port {{juno-port}} --dst-port {{osmosis-port}} --order unordered --version lvn-grant-001 --debug --override