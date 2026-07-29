# OpenAPI 3.1 Specification

The Lumenqraph API provides a machine-readable [OpenAPI 3.1](https://spec.openapis.org/oas/v3.1.0) specification for all REST endpoints, enabling automated client generation, interactive documentation, and API validation.

## Accessing the Specification

### JSON Specification
The OpenAPI schema is served at `/openapi.json`:

```bash
curl https://api.lumenqraph.io/openapi.json | jq .
```

### Interactive Documentation

#### Swagger UI
Explore and test the API with Swagger UI at `/docs`:
```
https://api.lumenqraph.io/docs
```

#### ReDoc
Browse the API documentation with ReDoc at `/redoc`:
```
https://api.lumenqraph.io/redoc
```

## API Endpoints

### System Endpoints

#### Health Check
```
GET /health
```
Returns the current service status and version.

**Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

#### Metrics
```
GET /metrics
```
Prometheus-format metrics for monitoring.

### Contract Discovery

#### List Contracts
```
GET /contracts
```
Returns all contracts with event counts and ledger range.

**Response:**
```json
[
  {
    "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
    "event_count": 15234,
    "first_seen_ledger": 1000000,
    "last_seen_ledger": 1050000
  }
]
```

#### Get Contract Interface
```
GET /contracts/{contract_id}/interface
```
Returns the decoded on-chain interface (functions, events, types) for a contract.

**Parameters:**
- `contract_id` (path, required): Soroban contract ID
- `version` (query, optional): Historical interface version

**Response:**
```json
{
  "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
  "has_events": true,
  "fetched_at": "2024-01-15T10:30:00Z",
  "interface": { ... }
}
```

### Contract Events

#### List Events
```
GET /contracts/{contract_id}/events
```
Retrieves recent events for a contract with optional filtering.

**Parameters:**
- `contract_id` (path, required): Soroban contract ID
- `limit` (query, optional): Max results (1-1000, default: 50)
- `offset` (query, optional): Pagination offset
- `after` (query, optional): Cursor for keyset pagination
- `event_name` (query, optional): Filter by event name (e.g., "transfer")

**Response:**
```json
{
  "data": [
    {
      "event_id": "...",
      "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
      "ledger": 1050000,
      "event_type": "contract",
      "event_name": "transfer",
      "topics": [...],
      "decoded_topics": [...],
      "value": "...",
      "decoded_value": {...}
    }
  ],
  "next_cursor": "..." // opaque cursor for next page
}
```

### Token Transfers

#### List Transfers
```
GET /contracts/{contract_id}/transfers
```
Returns token transfers derived from `transfer` events (SEP-41 compatible).

**Parameters:**
- `contract_id` (path, required): Soroban contract ID
- `limit` (query, optional): Max results (1-1000, default: 50)
- `offset` (query, optional): Pagination offset
- `after` (query, optional): Cursor for keyset pagination
- `from` (query, optional): Filter by sender address
- `to` (query, optional): Filter by recipient address

**Response:**
```json
{
  "data": [
    {
      "event_id": "...",
      "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
      "from_addr": "GXXXXXX...",
      "to_addr": "GYYYYYY...",
      "amount": "1000000000",
      "ledger": 1050000
    }
  ],
  "next_cursor": "..."
}
```

### Contract Data (State Snapshots)

#### List Contract Data
```
GET /contracts/{contract_id}/data
```
Returns the latest value of each tracked per-key storage entry.

**Parameters:**
- `contract_id` (path, required): Soroban contract ID
- `label` (query, optional): Filter by discovery label (e.g., "balance")
- `limit` (query, optional): Max keys (1-1000, default: 100)

**Response:**
```json
{
  "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
  "count": 5,
  "keys": [
    {
      "key_hash": "abc123...",
      "key": ["Balance", "GXXXXXX..."],
      "durability": "persistent",
      "ledger": 1050000,
      "value": "5000000000",
      "label": "balance",
      "captured_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

#### Get Contract Data History
```
GET /contracts/{contract_id}/data/{key_hash}
```
Returns the version history of a single storage entry.

**Parameters:**
- `contract_id` (path, required): Soroban contract ID
- `key_hash` (path, required): Hex SHA-256 hash of the storage key
- `limit` (query, optional): Max versions (1-500, default: 50)

**Response:**
```json
{
  "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
  "key_hash": "abc123...",
  "key": ["Balance", "GXXXXXX..."],
  "durability": "persistent",
  "count": 3,
  "versions": [
    {
      "ledger": 1050000,
      "value": "5000000000",
      "captured_at": "2024-01-15T10:30:00Z"
    },
    {
      "ledger": 1049900,
      "value": "4500000000",
      "captured_at": "2024-01-15T09:45:00Z"
    }
  ]
}
```

### Webhooks

#### List Webhooks
```
GET /webhooks
```
Returns all webhook subscriptions.

**Response:**
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "url": "https://example.com/webhooks",
    "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
    "event_name": "transfer",
    "active": true,
    "created_at": "2024-01-15T10:30:00Z"
  }
]
```

#### Create Webhook
```
POST /webhooks
Content-Type: application/json

{
  "url": "https://example.com/webhooks",
  "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
  "event_name": "transfer",
  "secret": "your-hmac-secret"
}
```

**Response:** (201 Created)
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "url": "https://example.com/webhooks",
  "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
  "event_name": "transfer",
  "active": true,
  "created_at": "2024-01-15T10:30:00Z"
}
```

#### Delete Webhook
```
DELETE /webhooks/{id}
```

**Response:** (204 No Content)

## Authentication

Most endpoints require an API key passed as:

```bash
curl -H "Authorization: Bearer YOUR_API_KEY" https://api.lumenqraph.io/contracts
```

Public endpoints (`/health`, `/metrics`, `/openapi.json`, `/docs`, `/redoc`) do not require authentication.

## Pagination

The API uses two pagination methods:

### Keyset (Cursor) Pagination (Recommended)
Provides constant-time performance regardless of dataset size:

```bash
# First page
curl 'https://api.lumenqraph.io/contracts/CXXX/events?limit=50'

# Next page using cursor from response
curl 'https://api.lumenqraph.io/contracts/CXXX/events?limit=50&after=OPAQUE_CURSOR'
```

### Offset Pagination (Deprecated)
Performance degrades with large offsets; not recommended:

```bash
# Page 1
curl 'https://api.lumenqraph.io/contracts/CXXX/events?limit=50&offset=0'

# Page 2
curl 'https://api.lumenqraph.io/contracts/CXXX/events?limit=50&offset=50'
```

## Error Responses

The API returns standard HTTP status codes with error details:

```json
{
  "error": "not_found",
  "message": "contract not found"
}
```

### Common Status Codes
- `200`: Success
- `201`: Created
- `204`: No Content (successful deletion)
- `400`: Bad Request (invalid parameters)
- `401`: Unauthorized (missing/invalid API key)
- `403`: Forbidden (rate limited)
- `404`: Not Found
- `429`: Too Many Requests (rate limit exceeded)
- `500`: Internal Server Error

## Rate Limiting

API rate limits are sent via headers:

```
X-RateLimit-Limit: 60
X-RateLimit-Remaining: 45
X-RateLimit-Reset: 1610000000
```

## GraphQL

The API also provides a GraphQL interface at `/graphql` with interactive IDE at `/graphql` (if introspection is enabled).

## Client Generation

Use the OpenAPI specification to generate clients for your language:

```bash
# JavaScript/TypeScript
openapi-generator-cli generate -i https://api.lumenqraph.io/openapi.json \
  -g typescript-axios -o ./generated-client

# Python
openapi-generator-cli generate -i https://api.lumenqraph.io/openapi.json \
  -g python -o ./generated-client

# Go
openapi-generator-cli generate -i https://api.lumenqraph.io/openapi.json \
  -g go -o ./generated-client
```

## References

- [OpenAPI 3.1 Specification](https://spec.openapis.org/oas/v3.1.0)
- [Swagger Editor](https://editor.swagger.io/)
- [OpenAPI Generator](https://openapi-generator.tech/)
- [Stellar Soroban Documentation](https://developers.stellar.org/docs)
