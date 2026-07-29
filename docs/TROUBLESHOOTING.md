# Troubleshooting & FAQ

This guide collects common operational issues, symptoms, causes, and step-by-step fixes for running Lumenqraph.

---

## 1. Why is `enriched` null in event payloads?

### Symptom
When querying `/contracts/:id/events` or the event stream, the JSON output contains `"enriched": null` instead of named, decoded fields (e.g., `{ "from": "...", "to": "...", "amount": "..." }`).

### Cause
Lumenqraph uses the contract's on-chain compiled interface spec (`contractspecv0`) to parse raw XDR into named fields. Enrichment fails (leaving `enriched` null) in these scenarios:
1. **Stellar Asset Contracts (SAC)**: Built-in token contracts (including the native XLM contract) do not have a custom WASM specification published on-chain.
2. **Missing Spec**: The contract was deployed without standard specification metadata, or the WASM bytecode is missing the spec section.
3. **Transient Fetch Failures**: The indexer failed to download the contract's WASM from the RPC during ingestion (e.g., due to rate limits or transient RPC downtime). The indexer caches `None` to prevent stalling the hot loop.
4. **Mismatched Event**: The event emitted does not match any event structure declared in the contract's specification.

### Fix
- Check if the contract is a Stellar Asset Contract (SAC). SAC token transfers are projected into the `/transfers` table regardless of spec presence.
- Inspect the database tables `contract_specs` and `contract_spec_versions` to see if a spec was indexed for the contract:
  ```sql
  SELECT contract_id, fetched_at, has_events FROM contract_specs WHERE contract_id = 'your-contract-id';
  ```
- Check indexer logs for `spec-fetching failures` or warnings. If a transient error occurred, restart the indexer or clear the cached entry in `contract_specs` to force a refetch:
  ```sql
  DELETE FROM contract_specs WHERE contract_id = 'your-contract-id';
  ```

---

## 2. Why did the indexer skip ahead / what is the "unrecoverable gap" warning?

### Symptom
Logs show the warning:
`unrecoverable gap in history detected, skipping ahead`
Or the processed ledger cursor jumps forward suddenly, leaving a gap in indexed events.

### Cause
Soroban RPC providers (especially public nodes) only retain events for a short window (typically ~7 days, or ~120,960 ledgers). If the indexer is stopped for a long time, or falls behind during heavy network activity:
1. The indexer requests events starting from its last processed ledger.
2. The RPC rejects the request with code `-32001` (processing limit / history limit exceeded).
3. To prevent stalling ingestion forever, the indexer applies the catch-up clamp controlled by `MAX_CATCHUP_LEDGERS`. It skips the missing history and starts indexing from the oldest available ledger in the RPC's retention window.

### Fix
- **Use a Retaining RPC**: Moving to a paid or self-hosted retaining RPC node is required for gapless indexing over long downtimes.
- **Adjust Configuration**: If your RPC provider supports deep history, increase the catch-up limit in your environment configuration:
  ```env
  MAX_CATCHUP_LEDGERS=120960   # Increase to match your RPC's retention window
  ```

---

## 3. Why isn't a contract callable via `/call?`

### Symptom
Making a `POST` request to `/contracts/:id/call` or `GET` to `/contracts/:id/functions` returns an error (such as `no on-chain interface indexed`).

### Cause
The `/call` endpoint simulates view functions read-only. To encode the input parameters and decode the response from the chain's `simulateTransaction` RPC, Lumenqraph must have a parsed contract spec. 
- Stellar Asset Contracts (SAC) do not publish a WASM spec, meaning their functions cannot be mapped dynamically.
- If the spec index is missing or could not be parsed, the endpoint rejects the call.

### Fix
- Ensure the contract target is a custom WASM contract with its spec published on-chain.
- Verify the spec is loaded by checking:
  ```bash
  curl http://localhost:8080/contracts/<CONTRACT_ID>/interface
  ```
- If the spec is missing, clear the db spec cache row to force the indexer to re-index it upon the next event ingestion (see Section 1).

---

## 4. Why is the `/contracts` endpoint slow?

### Symptom
Requests to `GET /contracts` take several seconds to load or result in gateway timeouts.

### Cause
In older versions, the `/contracts` endpoint performed a dynamic `GROUP BY` aggregation query over the entire `events` table (which can grow to millions of rows). This caused high CPU usage and slow responses.

### Fix
Lumenqraph now uses a dedicated `contract_summaries` table maintained in real-time by a database trigger (introduced in migration `0009_contract_summaries.sql`).
- Ensure all migrations are applied. The indexer applies them automatically on startup, but you can manually run them if using API-only nodes.
- Run database optimization commands on your Postgres instance:
  ```sql
  VACUUM ANALYZE contract_summaries;
  VACUUM ANALYZE events;
  ```

---

## 5. Database connection pool exhaustion and timeouts

### Symptom
Indexer or API logs show connection errors:
`sqlx::Error::PoolTimedOut` (failed to acquire connection from pool within timeout).

### Cause
Each Lumenqraph service spins up its own internal database pool. If the sum of maximum connections configured for all instances exceeds your managed database's concurrent limit, the database starts rejecting connections.
- Neon free tier cap: **25** concurrent connections.
- Supabase free tier cap: **60** concurrent connections.
- Render free Postgres cap: **25** concurrent connections.

### Fix
Configure the pool size ceiling using `DATABASE_MAX_CONNECTIONS` to stay safely below your database tier limit:
```env
# For a free Neon/Render DB (Max 25):
DATABASE_MAX_CONNECTIONS=3   # on indexer
DATABASE_MAX_CONNECTIONS=8   # on API
DATABASE_MAX_CONNECTIONS=2   # on webhooks
```
Increase limits only as you upgrade your database tiers.
