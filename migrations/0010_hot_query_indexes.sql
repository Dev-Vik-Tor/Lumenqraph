-- Audit and optimize indexes for hot query paths
-- This migration adds composite indexes for frequently accessed operations
-- as identified by the API route handlers.

-- Events: most critical hot path is list_events with optional event_name filter
-- Query pattern: WHERE contract_id = ? AND (event_name = ? OR NULL) ORDER BY ledger DESC, event_id DESC
-- Existing idx_events_contract_ledger(contract_id, ledger DESC) is good for the base case,
-- but we add a more specific composite index when event_name filtering is used.
CREATE INDEX IF NOT EXISTS idx_events_contract_event_ledger
    ON events (contract_id, event_name, ledger DESC, event_id DESC);

-- Token transfers: hot path is list_transfers with optional from/to address filters
-- Query pattern: WHERE contract_id = ? AND from_addr = ? AND to_addr = ? ORDER BY ledger DESC, event_id DESC
-- Add composite indexes for from_addr and to_addr filters combined with contract_id and ledger
CREATE INDEX IF NOT EXISTS idx_transfers_contract_from_ledger
    ON token_transfers (contract_id, from_addr, ledger DESC, event_id DESC);
CREATE INDEX IF NOT EXISTS idx_transfers_contract_to_ledger
    ON token_transfers (contract_id, to_addr, ledger DESC, event_id DESC);
-- Support the combined from + to filter case
CREATE INDEX IF NOT EXISTS idx_transfers_contract_from_to_ledger
    ON token_transfers (contract_id, from_addr, to_addr, ledger DESC, event_id DESC);

-- Contract data: DISTINCT ON (key_hash) pattern for listing latest per-key entries
-- Query pattern: DISTINCT ON (key_hash) WHERE contract_id = ? AND (label = ? OR NULL) ORDER BY key_hash, ledger DESC
-- The existing idx_contract_data_latest(contract_id, key_hash, ledger DESC) is already good
-- but we add label support for efficient label-filtered queries
CREATE INDEX IF NOT EXISTS idx_contract_data_contract_label_ledger
    ON contract_data (contract_id, label, ledger DESC);

-- Webhook enqueue operations: scan events.seq and contract_spec_versions.id ranges
-- These should already have indexes from earlier migrations, but verify they're optimized:
-- - events already has idx_events_seq (unique)
-- - contract_spec_versions benefits from an index on contract_id for version lookups
CREATE INDEX IF NOT EXISTS idx_contract_spec_versions_contract
    ON contract_spec_versions (contract_id);

-- Index on contract_spec_versions for fetching latest version efficiently
CREATE INDEX IF NOT EXISTS idx_contract_spec_versions_contract_version_desc
    ON contract_spec_versions (contract_id, version DESC);
