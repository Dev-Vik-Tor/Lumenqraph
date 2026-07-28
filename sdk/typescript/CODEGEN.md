# SDK type generation (#82)

The TypeScript SDK's response types are **generated from the canonical OpenAPI
schema** at `openapi.yaml` (repo root) using
[openapi-typescript](https://github.com/drwpow/openapi-typescript). This keeps
client and server in lockstep: adding or changing an API field requires
updating the schema, which immediately flows into the SDK's types.

## Workflow

```
openapi.yaml  ──codegen──▶  generated/api.d.ts  ──imported by──▶  src/index.ts
(source of truth)            (committed, generated)               (ergonomic client)
```

The generated types in `generated/api.d.ts` are **committed**. The client
wraps them with ergonomic named interfaces (`Contract`, `EventRecord`, etc.)
that re-export the relevant generated shapes.

## Commands

```bash
# Re-generate from the current openapi.yaml (run after editing the schema):
cd sdk/typescript
npm run codegen

# Verify the committed types match the schema (run in CI):
npm run codegen:check
```

## CI drift check

The CI workflow runs `npm run codegen:check` as part of the TypeScript SDK job.
It regenerates the types into a temporary file and diffs them against the
committed `generated/api.d.ts`. If they differ the job fails with:

```
❌ Generated types are stale!
   Run `npm run codegen` in sdk/typescript/ and commit the result.
```

This ensures that every PR that changes `openapi.yaml` must also commit the
regenerated types.

## Adding or changing API fields

1. Update `openapi.yaml` at the repo root.
2. Run `npm run codegen` in `sdk/typescript/`.
3. Update the ergonomic aliases in `src/index.ts` if the change affects an
   exported interface.
4. Commit both `openapi.yaml` and `generated/api.d.ts`.

## Future: live schema generation (#44)

Once `#44` (OpenAPI generation from the Axum router) lands, the schema source
of truth will move to the running API (`GET /openapi.json`). The codegen step
will then target that URL:

```bash
# After #44 lands:
npm run codegen  # reads http://localhost:8080/openapi.json
```

The CI drift check and the two-layer (generated + ergonomic) architecture will
remain unchanged.
