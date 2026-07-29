#!/usr/bin/env bash
# Populate a realistic demo dataset for local exploration without a live RPC.
# Usage: DATABASE_URL=... ./scripts/seed.sh
set -euo pipefail

: "${DATABASE_URL:=postgres://lumenqraph:lumenqraph@localhost:5432/lumenqraph}"

echo "Applying seed data to $DATABASE_URL..."
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f "$(dirname "$0")/seed.sql"
echo "Done! The explorer and API should now show meaningful data."
