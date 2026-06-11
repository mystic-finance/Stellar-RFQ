NETWORK ?= testnet
export NETWORK

.PHONY: all build test wasm fmt clean setup deploy seed-demo e2e mint fund

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

## Send test XLM gas to a wallet via Friendbot (testnet/futurenet only).
##   make fund TO=G...address
fund:
	@test -n "$(TO)" || { echo "Usage: make fund TO=<G...address>"; exit 1; }
	@case "$(NETWORK)" in \
	  testnet)   FB="https://friendbot.stellar.org" ;; \
	  futurenet) FB="https://friendbot-futurenet.stellar.org" ;; \
	  *) echo "Friendbot is testnet/futurenet only (NETWORK=$(NETWORK)); fund mainnet manually."; exit 1 ;; \
	esac; \
	echo "Requesting test XLM for $(TO) via Friendbot ($(NETWORK))..."; \
	RES=$$(curl -s "$$FB/?addr=$(TO)"); \
	if echo "$$RES" | grep -q '"hash"\|"successful"\|"_links"'; then \
	  echo "✓ Funded $(TO) (new accounts receive 10,000 test XLM)."; \
	else \
	  echo "$$RES" | jq -r '.detail // .title // "Friendbot request failed (already funded or invalid address?)."' 2>/dev/null \
	    || echo "Friendbot request failed (already funded or invalid address?)."; \
	fi
