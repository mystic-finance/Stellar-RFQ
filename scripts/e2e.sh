#!/usr/bin/env bash
# Convenience pipeline for a testnet deployment:
#   identities -> build -> deploy protocol -> seed demo tokens.
# The application/SDK integration (signing & filling orders) lives in the
# Octarine-UI and mystic-backend projects, not here.
source "$(dirname "$0")/lib.sh"

"$SCRIPT_DIR/00-setup.sh"
"$SCRIPT_DIR/01-build.sh"
"$SCRIPT_DIR/02-deploy.sh"
"$SCRIPT_DIR/03-seed-demo.sh"

log "Done. Deployment artifact: $DEPLOYMENT_FILE"
