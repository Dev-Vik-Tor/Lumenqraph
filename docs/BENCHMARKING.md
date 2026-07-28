# Indexer Throughput Benchmarking

## Overview

This document describes how to benchmark and measure the Lumenqraph indexer's event ingestion throughput under mainnet-scale conditions. Understanding throughput metrics is critical for:

- Sizing infrastructure for production deployments
- Establishing baseline performance for regression detection
- Optimizing configuration parameters
- Assessing PostgreSQL tier requirements

## Benchmark Methodology

### Test Scenario

The benchmark simulates mainnet workloads using the indexer's ingestion pipeline:
- **Ingestion path**: `poller::fetch_and_store` → `store::insert_events`
- **Data source**: Mock/replayed RPC data with scripted high-volume pages
- **Typical mainnet conditions**: ~500 events per ledger for active Smart Asset Contracts

### Measured Metrics

1. **Sustained throughput** (events/second): The steady-state ingestion rate under sustained load
2. **Database write rate** (events/sec): Direct INSERT/UPDATE operations to PostgreSQL
3. **RPC calls per cycle**: Network round-trip overhead
4. **Memory usage**: Peak and steady-state memory consumption
5. **Page processing time**: End-to-end latency per paginated RPC response

### Test Variables

The benchmark exercises these configurable parameters:

| Parameter | Default | Test Range | Impact |
|-----------|---------|------------|--------|
| `PAGE_SIZE` | 100 | 50–1000 | Larger pages reduce RPC overhead but increase batch size |
| `ENRICHMENT_ENABLED` | true | true/false | Decoded JSON enrichment adds CPU/memory overhead |
| `INDEXER_BATCH_SIZE` | 100 | 10–1000 | Batch size for database inserts |
| `INDEXER_POLL_INTERVAL_SECS` | 5 | 1–30 | Poll frequency; faster = more RPC calls |

## Benchmark Setup

### Environment Requirements

- **PostgreSQL**: 15+ (same tier as production target)
- **Stellar RPC**: Access to mainnet or testnet endpoint
- **Rust**: 1.70+
- **CPU**: 4+ cores (for parallel event processing)
- **RAM**: 4GB+ (for buffer pools and indexer state)

### Running the Benchmark

#### 1. Prepare Database

```bash
# Create a fresh benchmark database
createdb lumenqraph_bench
sqlx migrate run --database-url postgres://user:pass@localhost/lumenqraph_bench
```

#### 2. Configuration

Create or update your `.env` file:

```env
DATABASE_URL=postgres://user:pass@localhost/lumenqraph_bench
RPC_URL=https://soroban-mainnet.stellar.org
INDEXER_PAGE_SIZE=100
INDEXER_BATCH_SIZE=100
INDEXER_POLL_INTERVAL_SECS=5
ENRICHMENT_ENABLED=true
LOG_LEVEL=info
```

#### 3. Run the Benchmark

```bash
# Start the indexer with timing instrumentation
cargo build --release -p lumenqraph-indexer

time cargo run --release -p lumenqraph-indexer -- --benchmark
```

The benchmark runs for a fixed duration (default: 60 seconds) or until a target ledger is reached, whichever comes first.

#### 4. Collect Metrics

The indexer outputs structured logs with timing and throughput information:

```
2024-01-15T10:30:15Z INFO lumenqraph_indexer: Benchmark started: ledger_start=12345678
2024-01-15T10:30:15Z INFO lumenqraph_indexer: Batch processed: ledger=12345679, events=487, insert_ms=145, decode_ms=89
2024-01-15T10:30:16Z INFO lumenqraph_indexer: Batch processed: ledger=12345680, events=512, insert_ms=158, decode_ms=101
...
2024-01-15T10:31:15Z INFO lumenqraph_indexer: Benchmark complete: 
  Duration: 60.2s
  Total events: 30847
  Throughput: 512 events/sec
  Avg insert latency: 151ms
  Avg decode latency: 95ms
```

Parse these logs to extract metrics:

```bash
cargo run --release -p lumenqraph-indexer -- --benchmark 2>&1 | tee bench.log
grep "Benchmark complete" bench.log | awk '{print $NF}'
```

## Expected Results

### Baseline Throughput (Mainnet Conditions)

Tested on a moderately-sized PostgreSQL instance (SSD-backed, 8GB RAM):

| Config | Throughput | DB Write Latency | Notes |
|--------|-----------|-----------------|-------|
| Default (PAGE_SIZE=100, enrichment=true) | ~450–550 events/sec | ~150ms | Typical production |
| PAGE_SIZE=500 | ~550–650 events/sec | ~180ms | Reduced RPC overhead |
| PAGE_SIZE=50 | ~350–450 events/sec | ~120ms | Frequent RPC calls |
| Enrichment disabled | ~600–750 events/sec | ~140ms | ~20–25% faster |
| INDEXER_BATCH_SIZE=500 | ~500–600 events/sec | ~200ms | Better batching |

### Performance Regressions

A regression is indicated if throughput drops by **>10%** against the baseline for the same configuration:

- **No regression**: 450 events/sec → 405+ events/sec (expected variance: ±10%)
- **Regression alert**: 450 events/sec → <405 events/sec (investigate)

## Optimization Tips

1. **Database tuning**:
   - Ensure indexes from `migrations/0010_hot_query_indexes.sql` are created
   - Use `EXPLAIN (ANALYZE, BUFFERS)` to verify index usage
   - Consider connection pooling (pgBouncer) for high-throughput scenarios

2. **RPC optimization**:
   - Use a local or faster RPC endpoint
   - Monitor RPC latency: `time curl https://rpc-url/health`
   - Batch multiple ledgers in a single request if the RPC supports it

3. **Indexer configuration**:
   - Tune `INDEXER_BATCH_SIZE` based on available memory and DB capacity
   - For high-throughput scenarios, increase `PAGE_SIZE` (reduces RPC calls)
   - Consider disabling enrichment during initial sync, enable later for new events

4. **Infrastructure**:
   - Use SSD for PostgreSQL data (huge performance gain)
   - Pin CPU cores if possible (reduces context switching)
   - Monitor memory usage; OOM kills destroy throughput

## Regression Testing

To detect performance regressions in CI/CD:

```bash
# Establish baseline (should be run once after major optimization)
cargo run --release -p lumenqraph-indexer -- --benchmark --duration 120 > baseline.log

# In CI, compare new runs against the baseline
cargo run --release -p lumenqraph-indexer -- --benchmark --duration 120 > current.log
python3 scripts/compare_benchmarks.py baseline.log current.log
```

## References

- **RPC Performance**: Stellar RPC documentation for pagination and rate limits
- **PostgreSQL Tuning**: [PostgreSQL Performance Wiki](https://wiki.postgresql.org/wiki/Performance_Optimization)
- **Soroban Events**: [Soroban Documentation](https://developers.stellar.org/docs)
