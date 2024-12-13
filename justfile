set dotenv-load
export PERPS_WASM_DIR := "./wasm/artifacts"
GIT_SHA := `git rev-parse HEAD`

# List all recipies
default:
	just --list --unsorted

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
	env MARKET_COLLATERAL_TYPE={{collateral-type}} MARKET_TOKEN_KIND={{token-kind}} cargo nextest run --workspace --locked

# cargo tests check
cargo-test-check:
	just market-test base native
	# Commenting these tests to save time
	# just market-test quote native
	# just market-test base cw20
	just market-test quote cw20

# cargo tests check
cargo-test-check-local:
	just market-test base native
	just market-test quote native
	just market-test base cw20
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
# This is used for reproducible builds, suitable for mainnet
build-contracts:
	./.ci/contracts.sh

# Build contracts with Cosmos Docker tooling for arm64
# only for development purposes, not deploying mainnet contracts
# as per the docker tool's internal rules, these builds will have the architecture extension in the name
build-contracts-arm64:
	env OPTIMIZER_ARM64="true" ./.ci/contracts.sh

# Build contracts with native tooling
# only for development purposes, not deploying mainnet contracts
# the filenames are consolidated to be like regular docker builds so they can be
# deployed with our tooling easily
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
	-docker load -i ./wasm/images/workspace_0.16.1.tar
	-[ -f wasm/images/workspace_0.15.1.tar ] || docker pull cosmwasm/optimizer:0.16.1 && docker save cosmwasm/optimizer:0.16.1 > wasm/images/workspace_0.16.1.tar

# Typescript check for CI which needs deps installed
typescript-check:
	cd ts-schema && yarn install
	just typescript-schema

# Typescript schema
typescript-schema:
	cd packages/perpswap && cargo run --example generate-schema
	cd ts-schema && yarn && yarn build

# Generate the schema and copy to a webapp directory located at ../webapp
typescript-schema-copy: typescript-schema
	rm -rf ../webapp/apps/perps/src/apps/perps/sdk/types/.generated/
	mv schema/typescript ../webapp/apps/perps/src/apps/perps/sdk/types/.generated/

# Build perps-qa binary in release mode
cargo-release:
    cargo build --bin perps-qa --release --bins --locked

# Build bots binary in release mode
cargo-bots-release:
    cargo build --bin perps-bots --release --target x86_64-unknown-linux-musl

# Build bots binary in release mode
cargo-bots-release-arm:
    cross build --bin perps-bots --release --target aarch64-unknown-linux-musl

# Build bots docker image
build-bots-image:
	rm -rf .ci/bots/etc
	cp -r packages/perps-exes/assets .ci/bots/etc
	cp target/aarch64-unknown-linux-musl/release/perps-bots .ci/bots
	cd .ci/bots && docker image build . -f Dockerfile -t ghcr.io/levana-protocol/levana-perps/bots:{{GIT_SHA}} --platform linux/arm64

# Push bots docker image
push-bots-image:
	docker push ghcr.io/levana-protocol/levana-perps/bots:{{GIT_SHA}}

# Build companion binary in release mode
cargo-companion-release-arm:
    cross build --bin perps-companion --release --target aarch64-unknown-linux-musl

# Build companion docker image
build-companion-image:
	cp ./packages/perps-exes/assets/mainnet-factories.toml .ci/companion/
	cp target/aarch64-unknown-linux-musl/release/perps-companion .ci/companion
	cd .ci/companion && docker buildx build . --file Dockerfile -t ghcr.io/levana-protocol/levana-perps/companion:{{GIT_SHA}} --platform linux/arm64

# Push bots docker image
push-companion-image:
	docker push ghcr.io/levana-protocol/levana-perps/companion:{{GIT_SHA}}

# Run companion
run-companion:
	cargo run --bin perps-companion

# Deploy to Osmosis tesntet
deploy-osmosis-ci:
	cargo run --bin perps-deploy testnet store-code --network osmosis-testnet
	cargo run --bin perps-deploy testnet instantiate --family osmoci

# Migrate osmoci
migrate-osmoci:
	cargo run --bin perps-deploy testnet store-code --network osmosis-testnet
	cargo run --bin perps-deploy testnet migrate --family osmoci

# Build documentations
build-docs:
	mkdir -p ./.output/temp/schema/cosmos
	cargo doc --no-deps --package levana_perpswap_cosmos --target-dir=./.output/temp/
	cp ./.ci/index.html ./.output/temp/doc/index.html
	echo "<html><body><h1>404 page not found</h1></body></html>" > ./.output/temp/doc/404.html

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

rewards-mint-test owner:
	cargo run --bin rewards-test mint-test --hatch-network=juno-testnet --owner={{owner}}

# Rewards subcommands
store-hatching:
	cargo run --bin perps-deploy testnet store-code --contracts=hatching --network=juno-testnet
store-lvn-rewards:
	cargo run --bin perps-deploy testnet store-code --contracts=lvn-rewards --network=osmosis-testnet
instantiate-hatching:
	cargo run --bin perps-deploy testnet instantiate-rewards --contracts=hatching --network=juno-testnet
instantiate-lvn-rewards:
	cargo run --bin perps-deploy testnet instantiate-rewards --contracts=lvn-rewards --network=osmosis-testnet
store-ibc-execute:
	cargo run --bin perps-deploy testnet store-code --contracts=ibc-execute-proxy --network=stargaze-testnet
instantiate-nft-mint:
	cargo run --bin perps-deploy testnet instantiate-rewards --contracts=ibc-execute-proxy --ibc-execute-proxy-target=nft-mint --network=stargaze-testnet
migrate-hatching:
	cargo run --bin perps-deploy testnet migrate-rewards --contracts=hatching --network=juno-testnet
hatch-egg-test:
	cargo run --bin rewards-test hatch-egg --hatch-network=juno-testnet --nft-mint-network=stargaze-testnet --lvn-rewards-network=osmosis-testnet
create-nft-mint-relayer-channel path-name juno-port stargaze-port:
	rly transact channel {{path-name}} --src-port {{juno-port}} --dst-port {{stargaze-port}} --order unordered --version nft-mint-001 --debug --override
create-lvn-grant-relayer-channel path-name juno-port osmosis-port:
	rly transact channel {{path-name}} --src-port {{juno-port}} --dst-port {{osmosis-port}} --order unordered --version lvn-grant-001 --debug --override

# Build perps-market-params binary in release mode
cargo-market-params-arm:
    cross build --bin perps-market-params --release --target aarch64-unknown-linux-musl

# Build perps-market-params docker image
build-market-params-image:
	cp ./packages/perps-exes/assets/mainnet-factories.toml .ci/market-analyzer/
	cp target/aarch64-unknown-linux-musl/release/perps-market-params .ci/market-analyzer/
	cd .ci/market-analyzer && docker image build . -f Dockerfile -t ghcr.io/levana-protocol/levana-perps/perps-market-params:{{GIT_SHA}} --platform linux/arm64

# Push perps-market-params docker image
push-market-params-image:
	docker push ghcr.io/levana-protocol/levana-perps/perps-market-params:{{GIT_SHA}}

# Build perps-deploy binary in release mode
cargo-perps-deploy-release:
    cargo build --bin perps-deploy --release --target aarch64-unknown-linux-musl

# Build perps-deploy docker image
build-perps-deploy-image:
	cp ./packages/perps-exes/assets/mainnet-factories.toml .ci/perps-deploy/
	cp target/aarch64-unknown-linux-musl/release/perps-deploy .ci/perps-deploy/
	cd .ci/perps-deploy && docker image build . -f Dockerfile -t ghcr.io/levana-protocol/levana-perps/perps-deploy:{{GIT_SHA}}

# Push perps-deploy docker image
push-perps-deploy-image:
	docker push ghcr.io/levana-protocol/levana-perps/perps-deploy:{{GIT_SHA}}
