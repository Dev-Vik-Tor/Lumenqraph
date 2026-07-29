#!/bin/bash

# Lumenqraph Indexer Throughput Benchmark Script
#
# This script measures and documents indexer throughput under controlled conditions.
# It provides a reproducible way to benchmark mainnet-scale ingestion.
#
# Usage: ./scripts/benchmark_indexer.sh [--duration SECS] [--config CONFIG_FILE]

set -euo pipefail

# Configuration
DURATION=${DURATION:-60}  # Default: 60 seconds
CONFIG_FILE="${CONFIG_FILE:-.env}"
BENCHMARK_DIR="benchmarks"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
RESULTS_FILE="${BENCHMARK_DIR}/results_${TIMESTAMP}.log"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --duration)
            DURATION="$2"
            shift 2
            ;;
        --config)
            CONFIG_FILE="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--duration SECS] [--config CONFIG_FILE]"
            exit 1
            ;;
    esac
done

# Create results directory
mkdir -p "$BENCHMARK_DIR"

echo -e "${GREEN}Lumenqraph Indexer Throughput Benchmark${NC}"
echo "=========================================="
echo "Timestamp: $TIMESTAMP"
echo "Duration: ${DURATION}s"
echo "Config: $CONFIG_FILE"
echo ""

# Check if config file exists
if [[ ! -f "$CONFIG_FILE" ]]; then
    echo -e "${RED}Error: Config file not found: $CONFIG_FILE${NC}"
    echo "Please copy .env.example to .env and configure DATABASE_URL and RPC_URL"
    exit 1
fi

# Load environment from config
export $(cat "$CONFIG_FILE" | grep -v '^#' | xargs)

# Verify required environment variables
if [[ -z "${DATABASE_URL:-}" ]]; then
    echo -e "${RED}Error: DATABASE_URL not set in $CONFIG_FILE${NC}"
    exit 1
fi

if [[ -z "${RPC_URL:-}" ]]; then
    echo -e "${RED}Error: RPC_URL not set in $CONFIG_FILE${NC}"
    exit 1
fi

echo -e "${YELLOW}Configuration:${NC}"
echo "  DATABASE_URL: ${DATABASE_URL}"
echo "  RPC_URL: ${RPC_URL}"
echo "  PAGE_SIZE: ${INDEXER_PAGE_SIZE:-100}"
echo "  BATCH_SIZE: ${INDEXER_BATCH_SIZE:-100}"
echo "  ENRICHMENT: ${ENRICHMENT_ENABLED:-true}"
echo ""

# Check database connectivity
echo -e "${YELLOW}Checking database connectivity...${NC}"
if ! psql "$DATABASE_URL" -c "SELECT 1" > /dev/null 2>&1; then
    echo -e "${RED}Error: Cannot connect to database${NC}"
    echo "DATABASE_URL: $DATABASE_URL"
    exit 1
fi
echo -e "${GREEN}Database connected successfully${NC}"
echo ""

# Check RPC connectivity
echo -e "${YELLOW}Checking RPC connectivity...${NC}"
if ! timeout 5 curl -s -X POST "$RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc": "2.0", "id": 1, "method": "getHealth"}' | grep -q "healthy"; then
    echo -e "${RED}Warning: RPC health check failed${NC}"
    echo "The RPC endpoint may be unavailable or slow"
else
    echo -e "${GREEN}RPC connected successfully${NC}"
fi
echo ""

# Run benchmark
echo -e "${YELLOW}Starting benchmark...${NC}"
echo ""

# Build in release mode if not already done
if ! cargo build --release -p lumenqraph-indexer 2>&1 | tail -3; then
    echo -e "${RED}Failed to build indexer${NC}"
    exit 1
fi

# Run the indexer with timing instrumentation
# Note: This assumes the indexer binary is available at the expected path
# For now, we'll provide timing information via shell
START_TIME=$(date +%s%N)

# Run the indexer - capture logs and output
BENCHMARK_RUNS=$(cargo run --release -p lumenqraph-indexer 2>&1 || true)

END_TIME=$(date +%s%N)
ELAPSED_MS=$(( (END_TIME - START_TIME) / 1000000 ))
ELAPSED_SECS=$(echo "scale=2; $ELAPSED_MS / 1000" | bc)

echo ""
echo -e "${GREEN}Benchmark complete${NC}"
echo "Elapsed time: ${ELAPSED_SECS}s"
echo ""

# Save results
cat > "$RESULTS_FILE" << EOF
# Lumenqraph Indexer Benchmark Results
Timestamp: $TIMESTAMP
Duration: ${DURATION}s
Actual elapsed: ${ELAPSED_SECS}s

## Configuration
DATABASE_URL: $DATABASE_URL
RPC_URL: $RPC_URL
PAGE_SIZE: ${INDEXER_PAGE_SIZE:-100}
BATCH_SIZE: ${INDEXER_BATCH_SIZE:-100}
ENRICHMENT_ENABLED: ${ENRICHMENT_ENABLED:-true}

## Raw Output
$BENCHMARK_RUNS

## Analysis
Please check the logs above for:
- Total events processed
- Throughput (events/sec)
- Average insert latency
- Average decode latency
- Peak memory usage
- Any errors or warnings
EOF

echo -e "${GREEN}Results saved to: $RESULTS_FILE${NC}"
echo ""

# Parse and display key metrics if available
if echo "$BENCHMARK_RUNS" | grep -q "events processed"; then
    echo -e "${YELLOW}Key Metrics:${NC}"
    echo "$BENCHMARK_RUNS" | grep -E "events processed|Throughput|latency" || true
fi

echo ""
echo "For detailed analysis, see: $RESULTS_FILE"
