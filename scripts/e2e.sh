#!/usr/bin/env bash
# Convenience pipeline for a testnet deployment:
#   identities -> build -> deploy protocol -> seed demo tokens -> oracle -> fills.
# Production application/SDK integration lives in the Octarine-UI and
# mystic-backend projects; 05-fill-demo.sh here is a settlement smoke test.
source "$(dirname "$0")/lib.sh"

"$SCRIPT_DIR/00-setup.sh"
"$SCRIPT_DIR/01-build.sh"
"$SCRIPT_DIR/02-deploy.sh"
"$SCRIPT_DIR/03-seed-demo.sh"
"$SCRIPT_DIR/04-oracle.sh"
"$SCRIPT_DIR/05-fill-demo.sh"

log "Done. Deployment artifact: $DEPLOYMENT_FILE"
