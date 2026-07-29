-- Demo dataset / seed script for local exploration without a live RPC.
-- Inserts realistic decoded events, interfaces, state, and data so the
-- explorer and API show meaningful content immediately after seeding.
--
-- Usage: psql $DATABASE_URL -f scripts/seed.sql

BEGIN;

-- Ensure cursor exists (required for indexer operation).
INSERT INTO indexer_cursor (id, last_processed_ledger, updated_at)
VALUES (1, 1000000, NOW())
ON CONFLICT (id) DO UPDATE
    SET last_processed_ledger = 1000000,
        updated_at = NOW();

-- Demo contract: SEP-41 token (fictional USDC-like stablecoin).
INSERT INTO events (
    event_id, contract_id, ledger, ledger_closed_at, event_type,
    topics, event_name, value, tx_hash, in_successful_call, paging_token, created_at
) VALUES
-- Transfer event
('0000001-0000000001', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 1000000, NOW() - INTERVAL '1 hour',
 'contract', '["AAAADwAAAAh0cmFuc2Zlcg=="]', 'transfer',
 'AAAAEQAAAAEAAAACAAAAEgAAAAAAAAAAfn0jtXvOQK5+p8H7mZYyxmjQKDZd1v1dw1DqPBRPzDAAAAASAAAAAAAAAABXVaE6fY9F8v7DOC1+v2cF6wZXpKTNRjKDZPrJpLj4AA==',
 'abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890', true,
 '1000000-1', NOW() - INTERVAL '1 hour'),

-- Mint event
('0000001-0000000002', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 1000001, NOW() - INTERVAL '50 minutes',
 'contract', '["AAAADwAAAARtaW50"]', 'mint',
 'AAAAEQAAAAEAAAACAAAAEgAAAAAAAAAAV1WhOn2PRfL+wzgtfr9nBesGV6SkzUYyg2T6yaS4+AAAAA4AAAAQMTAwMDAwMDAwMDAwMDAwMA==',
 'fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321', true,
 '1000001-1', NOW() - INTERVAL '50 minutes'),

-- Burn event
('0000001-0000000003', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 1000002, NOW() - INTERVAL '40 minutes',
 'contract', '["AAAADwAAAARidXJu"]', 'burn',
 'AAAAEQAAAAEAAAACAAAAEgAAAAAAAAAAfn0jtXvOQK5+p8H7mZYyxmjQKDZd1v1dw1DqPBRPzDAAAAAOAAAADzUwMDAwMDAwMDAwMDAwMA==',
 'aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899', true,
 '1000002-1', NOW() - INTERVAL '40 minutes'),

-- Approval event
('0000001-0000000004', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 1000003, NOW() - INTERVAL '30 minutes',
 'contract', '["AAAADwAAAAhhcHByb3Zl"]', 'approve',
 'AAAAEQAAAAEAAAADAAAAEgAAAAAAAAAAfn0jtXvOQK5+p8H7mZYyxmjQKDZd1v1dw1DqPBRPzDAAAAAOAAAADzEwMDAwMDAwMDAwMDAwMDAAAAAKAAAAAAAAAAAAAAARmN6/ag==',
 '1122334455667788991122334455667788991122334455667788991122334455', true,
 '1000003-1', NOW() - INTERVAL '30 minutes');

-- Demo contract interface: SEP-41 token spec.
INSERT INTO contract_specs (
    contract_id, wasm_hash, interface, spec_section, has_events, fetched_at
) VALUES
('CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM',
 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
 '{
   "functions": [
     {"name": "transfer", "inputs": [{"name": "from", "type": "Address"}, {"name": "to", "type": "Address"}, {"name": "amount", "type": "i128"}], "outputs": []},
     {"name": "mint", "inputs": [{"name": "to", "type": "Address"}, {"name": "amount", "type": "i128"}], "outputs": []},
     {"name": "burn", "inputs": [{"name": "from", "type": "Address"}, {"name": "amount", "type": "i128"}], "outputs": []},
     {"name": "approve", "inputs": [{"name": "from", "type": "Address"}, {"name": "spender", "type": "Address"}, {"name": "amount", "type": "i128"}, {"name": "expiration_ledger", "type": "u32"}], "outputs": []},
     {"name": "balance", "inputs": [{"name": "id", "type": "Address"}], "outputs": [{"type": "i128"}]}
   ],
   "events": [
     {"name": "transfer", "fields": [{"name": "from", "type": "Address"}, {"name": "to", "type": "Address"}, {"name": "amount", "type": "i128"}]},
     {"name": "mint", "fields": [{"name": "to", "type": "Address"}, {"name": "amount", "type": "i128"}]},
     {"name": "burn", "fields": [{"name": "from", "type": "Address"}, {"name": "amount", "type": "i128"}]},
     {"name": "approve", "fields": [{"name": "from", "type": "Address"}, {"name": "spender", "type": "Address"}, {"name": "amount", "type": "i128"}, {"name": "expiration_ledger", "type": "u32"}]}
   ]
 }',
 '', true, NOW());

-- Demo contract state: token metadata.
INSERT INTO contract_state (
    contract_id, ledger, storage, captured_at
) VALUES
('CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 1000000,
 '{
   "name": "Demo USDC",
   "symbol": "USDC",
   "decimals": 7,
   "admin": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4"
 }',
 NOW() - INTERVAL '1 hour');

-- Demo contract data: per-holder balances.
INSERT INTO contract_data (
    contract_id, key_hash, key, key_xdr, durability, ledger, value, label, captured_at
) VALUES
-- Balance for holder 1
('CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM',
 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
 '["Balance", "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4"]',
 'AAAADwAAAAdCYWxhbmNlAAAAABIAAAAAAAAAAH59I7V7zkCufqfB+5mWMsZo0Cg2Xdb9XcNQ6jwUT8ww',
 'persistent', 1000003,
 '"5000000000000000"', 'balance', NOW() - INTERVAL '30 minutes'),

-- Balance for holder 2
('CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM',
 'a1b2c3d4e5f6708192a3b4c5d6e7f8091a2b3c4d5e6f7081920a3b4c5d6e7f80',
 '["Balance", "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAC3I7"]',
 'AAAADwAAAAdCYWxhbmNlAAAAABIAAAAAAAAAAFdVoTp9j0Xy/sM4LX6/ZwXrBlekpM1GMoNk+smkuPgA',
 'persistent', 1000003,
 '"10000000000000000"', 'balance', NOW() - INTERVAL '30 minutes');

COMMIT;

SELECT 'Seed data inserted successfully!' AS status,
       (SELECT COUNT(*) FROM events) AS events_count,
       (SELECT COUNT(*) FROM contract_specs) AS specs_count,
       (SELECT COUNT(*) FROM contract_state) AS state_count,
       (SELECT COUNT(*) FROM contract_data) AS data_count;
