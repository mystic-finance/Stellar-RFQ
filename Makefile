NETWORK ?= testnet
export NETWORK

.PHONY: all build test wasm fmt clean setup deploy seed-demo e2e mint

all: build test

## Build the contract for host (debug) — used by tests.
build:
	cargo build

## Run the contract unit-test suite.
test:
	cargo test -p rfq

## Build the on-chain WASM and optimise it.
wasm:
	./scripts/01-build.sh

fmt:
	cargo fmt

clean:
	cargo clean

## Generate + fund identities (admin/maker/taker).
setup:
	./scripts/00-setup.sh

## Deploy the protocol contract (prebuilt wasm) -> deployments/$(NETWORK).json.
deploy:
	./scripts/02-deploy.sh

## Testnet-only: deploy test tokens, mint, register signer.
seed-demo:
	./scripts/03-seed-demo.sh

## Run the testnet pipeline: setup -> build -> deploy -> seed-demo.
e2e:
	./scripts/e2e.sh

## Mint test tokens (RFQA + RFQB) to an address (admin-gated).
##   make mint TO=G...address [AMOUNT=10000000000]   # amount in raw units, 7 decimals
mint:
	@test -n "$(TO)" || { echo "Usage: make mint TO=<G...address> [AMOUNT=10000000000]"; exit 1; }
	@RFQA=$$(jq -r .contracts.tokenA deployments/$(NETWORK).json); \
	RFQB=$$(jq -r .contracts.tokenB deployments/$(NETWORK).json); \
	AMT=$${AMOUNT:-10000000000}; \
	echo "Minting $$AMT of RFQA ($$RFQA) and RFQB ($$RFQB) to $(TO) on $(NETWORK)..."; \
	stellar contract invoke --id $$RFQA --source rfq-admin --network $(NETWORK) -- mint --to $(TO) --amount $$AMT; \
	stellar contract invoke --id $$RFQB --source rfq-admin --network $(NETWORK) -- mint --to $(TO) --amount $$AMT; \
	echo "Minted RFQA + RFQB to $(TO)."
