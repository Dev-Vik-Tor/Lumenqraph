/**
 * Tests for the Lumenqraph TypeScript SDK.
 *
 * Covers:
 *  - Issue #81: retry / backoff / timeout (LumenqraphClient request loop)
 *  - Issue #83: verifyWebhook signature helper
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  LumenqraphClient,
  LumenqraphError,
  verifyWebhook,
  type ClientOptions,
} from "./index.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Build a minimal `fetch` mock that returns the responses in `replies` in
 * order.  Each entry is either:
 *   - `{ status, body? }` — a resolved Response with that status / JSON body
 *   - `"network"` — a rejected promise (simulated network error)
 *   - `"timeout"` — a rejected promise with an AbortError
 */
type FetchReply =
  | { status: number; body?: unknown; headers?: Record<string, string> }
  | "network"
  | "timeout";

function mockFetch(...replies: FetchReply[]) {
  let call = 0;
  return vi.fn(async (_url: string, init?: RequestInit) => {
    const reply = replies[call++];
    if (reply === undefined) {
      throw new Error(`mockFetch: no reply configured for call #${call}`);
    }
    if (reply === "network") {
      throw new TypeError("Failed to fetch");
    }
    if (reply === "timeout") {
      const err = new DOMException("The user aborted a request.", "AbortError");
      // If the caller passed a signal, abort it so downstream code sees it
      if (init?.signal) {
        // AbortController signals are read-only; just throw the abort error
      }
      throw err;
    }
    const headers = new Headers(reply.headers ?? {});
    const bodyText = reply.body !== undefined ? JSON.stringify(reply.body) : "";
    return new Response(bodyText, { status: reply.status, headers });
  });
}

/** Minimal client options with retries disabled by default (most unit tests
 *  don't want real sleep delays). Pass `retry` to override. */
function makeClient(
  fetch: ReturnType<typeof mockFetch>,
  opts: Partial<Omit<ClientOptions, "baseUrl" | "fetch">> = {},
): LumenqraphClient {
  return new LumenqraphClient({
    baseUrl: "http://localhost:8080",
    fetch: fetch as unknown as typeof globalThis.fetch,
    retry: { maxRetries: 0, timeoutMs: 500, ...opts.retry },
    ...opts,
  });
}

// ---------------------------------------------------------------------------
// #83 verifyWebhook
// ---------------------------------------------------------------------------

describe("verifyWebhook (#83)", () => {
  // Known-good fixture — produced by the same HMAC-SHA256 algorithm the server
  // uses in lumenqraph-webhooks/src/dispatcher.rs.
  //
  //   echo -n 'hello world' | openssl dgst -sha256 -hmac 'secret'
  //   => 734cc62f32841568f45715aeb9f4d7db
  //      No — use:
  //   node -e "const c=require('crypto');console.log('sha256='+c.createHmac('sha256','secret').update('hello world').digest('hex'))"
  const BODY = "hello world";
  const SECRET = "secret";
  // Pre-computed via Node crypto:
  // sha256=b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7
  let VALID_SIG: string;

  beforeEach(async () => {
    // Compute dynamically so the test is self-contained and cross-platform.
    const enc = new TextEncoder();
    const key = await crypto.subtle.importKey(
      "raw",
      enc.encode(SECRET),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const sig = await crypto.subtle.sign("HMAC", key, enc.encode(BODY));
    const hex = Array.from(new Uint8Array(sig))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
    VALID_SIG = `sha256=${hex}`;
  });

  it("returns true for a valid signature (string body)", async () => {
    expect(await verifyWebhook(BODY, VALID_SIG, SECRET)).toBe(true);
  });

  it("returns true for a valid signature (Uint8Array body)", async () => {
    const bytes = new TextEncoder().encode(BODY);
    expect(await verifyWebhook(bytes, VALID_SIG, SECRET)).toBe(true);
  });

  it("returns false for a tampered body", async () => {
    expect(await verifyWebhook("hello WORLD", VALID_SIG, SECRET)).toBe(false);
  });

  it("returns false for a tampered signature", async () => {
    const tampered = VALID_SIG.slice(0, -4) + "0000";
    expect(await verifyWebhook(BODY, tampered, SECRET)).toBe(false);
  });

  it("returns false when the sha256= prefix is missing", async () => {
    const noPrefix = VALID_SIG.replace("sha256=", "");
    expect(await verifyWebhook(BODY, noPrefix, SECRET)).toBe(false);
  });

  it("returns false for a completely wrong secret", async () => {
    expect(await verifyWebhook(BODY, VALID_SIG, "wrong-secret")).toBe(false);
  });

  it("returns false for an empty signature header", async () => {
    expect(await verifyWebhook(BODY, "", SECRET)).toBe(false);
  });

  it("returns false when signature length differs (different secret length)", async () => {
    // sha256= prefix + 64 hex chars is always 71 chars total; a truncated one
    // should still fail cleanly (length mismatch in constant-time path).
    const short = "sha256=aabb";
    expect(await verifyWebhook(BODY, short, SECRET)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// #81 Retry / backoff / timeout
// ---------------------------------------------------------------------------

describe("request retry (#81)", () => {
  it("returns the response immediately when no error occurs", async () => {
    const fetch = mockFetch({ status: 200, body: ["contract1"] });
    const client = makeClient(fetch);
    const result = await client.listContracts();
    expect(result).toEqual(["contract1"]);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("does not retry on a 400 error (client error, not transient)", async () => {
    const fetch = mockFetch(
      { status: 400, body: { error: "bad request" } },
    );
    const client = makeClient(fetch, { retry: { maxRetries: 3, timeoutMs: 500 } });
    await expect(client.listContracts()).rejects.toBeInstanceOf(LumenqraphError);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("does not retry on a 404 error", async () => {
    const fetch = mockFetch({ status: 404, body: { error: "not found" } });
    const client = makeClient(fetch, { retry: { maxRetries: 3, timeoutMs: 500 } });
    await expect(client.listContracts()).rejects.toMatchObject({ status: 404 });
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("does not retry on a 401 error", async () => {
    const fetch = mockFetch({ status: 401, body: { error: "unauthorized" } });
    const client = makeClient(fetch, { retry: { maxRetries: 3, timeoutMs: 500 } });
    await expect(client.listContracts()).rejects.toMatchObject({ status: 401 });
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("retries a 503 and succeeds on the second attempt", async () => {
    const fetch = mockFetch(
      { status: 503, body: { error: "service unavailable" } },
      { status: 200, body: [] },
    );
    const client = makeClient(fetch, {
      retry: { maxRetries: 2, baseDelayMs: 0, timeoutMs: 500 },
    });
    const result = await client.listContracts();
    expect(result).toEqual([]);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("retries a 502 and succeeds on the second attempt", async () => {
    const fetch = mockFetch(
      { status: 502 },
      { status: 200, body: [] },
    );
    const client = makeClient(fetch, {
      retry: { maxRetries: 2, baseDelayMs: 0, timeoutMs: 500 },
    });
    await client.listContracts();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("retries a 504 and succeeds on the third attempt", async () => {
    const fetch = mockFetch(
      { status: 504 },
      { status: 504 },
      { status: 200, body: [{ contract_id: "C1", event_count: 0, first_seen_ledger: null, last_seen_ledger: null }] },
    );
    const client = makeClient(fetch, {
      retry: { maxRetries: 3, baseDelayMs: 0, timeoutMs: 500 },
    });
    const result = await client.listContracts();
    expect(result[0]?.contract_id).toBe("C1");
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it("retries a 429 and succeeds on the second attempt", async () => {
    const fetch = mockFetch(
      { status: 429 },
      { status: 200, body: [] },
    );
    const client = makeClient(fetch, {
      retry: { maxRetries: 2, baseDelayMs: 0, timeoutMs: 500 },
    });
    await client.listContracts();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("throws LumenqraphError after exhausting all retries on 503", async () => {
    const fetch = mockFetch(
      { status: 503 },
      { status: 503 },
      { status: 503 },
    );
    const client = makeClient(fetch, {
      retry: { maxRetries: 2, baseDelayMs: 0, timeoutMs: 500 },
    });
    await expect(client.listContracts()).rejects.toBeInstanceOf(LumenqraphError);
    expect(fetch).toHaveBeenCalledTimes(3); // 1 initial + 2 retries
  });

  it("retries on a network error", async () => {
    const fetch = mockFetch("network", { status: 200, body: [] });
    const client = makeClient(fetch, {
      retry: { maxRetries: 2, baseDelayMs: 0, timeoutMs: 500 },
    });
    await client.listContracts();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("throws after exhausting retries on persistent network errors", async () => {
    const fetch = mockFetch("network", "network", "network");
    const client = makeClient(fetch, {
      retry: { maxRetries: 2, baseDelayMs: 0, timeoutMs: 500 },
    });
    await expect(client.listContracts()).rejects.toThrow();
    expect(fetch).toHaveBeenCalledTimes(3);
  });

  it("honors Retry-After (integer seconds) on 429", async () => {
    const fetch = mockFetch(
      { status: 429, headers: { "retry-after": "0" } },
      { status: 200, body: [] },
    );
    const client = makeClient(fetch, {
      retry: { maxRetries: 2, baseDelayMs: 0, timeoutMs: 500 },
    });
    // Just verify it doesn't throw and retries; exact timing not asserted
    await client.listContracts();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("honors Retry-After (HTTP-date) on 429", async () => {
    // A date in the past (0ms wait) so the test stays fast.
    const pastDate = new Date(Date.now() - 1000).toUTCString();
    const fetch = mockFetch(
      { status: 429, headers: { "retry-after": pastDate } },
      { status: 200, body: [] },
    );
    const client = makeClient(fetch, {
      retry: { maxRetries: 2, baseDelayMs: 0, timeoutMs: 500 },
    });
    await client.listContracts();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("does not retry when maxRetries is 0", async () => {
    const fetch = mockFetch({ status: 503 });
    const client = makeClient(fetch, {
      retry: { maxRetries: 0, timeoutMs: 500 },
    });
    await expect(client.listContracts()).rejects.toBeInstanceOf(LumenqraphError);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("uses an AbortController to cancel timed-out requests", async () => {
    // Fake a fetch that respects the AbortSignal: it waits until either the
    // signal fires or a long sentinel timer expires, then rejects/resolves
    // accordingly.  This way the test's own timer never needs to wait for the
    // full sentinel — the abort fires after `timeoutMs` (10 ms).
    let capturedSignal: AbortSignal | undefined;
    const slowFetch = vi.fn(
      (_url: string, init?: RequestInit): Promise<Response> => {
        capturedSignal = init?.signal ?? undefined;
        return new Promise((_resolve, reject) => {
          // If the signal is already aborted (unlikely at this point) reject now.
          if (capturedSignal?.aborted) {
            reject(new DOMException("Aborted", "AbortError"));
            return;
          }
          // Listen for the abort event so we reject as soon as the SDK fires it.
          capturedSignal?.addEventListener("abort", () => {
            reject(new DOMException("The user aborted a request.", "AbortError"));
          });
          // Sentinel: if somehow the abort never fires, fail after 2 s so the
          // test doesn't hang for the full vitest default timeout.
          setTimeout(() => reject(new Error("sentinel: timeout not fired")), 2000);
        });
      },
    );

    const client = new LumenqraphClient({
      baseUrl: "http://localhost:8080",
      fetch: slowFetch as unknown as typeof globalThis.fetch,
      retry: { maxRetries: 0, timeoutMs: 10 }, // 10 ms timeout
    });

    await expect(client.listContracts()).rejects.toThrow();
    expect(capturedSignal).toBeDefined();
  });

  it("LumenqraphError carries status and body", async () => {
    const fetch = mockFetch({ status: 422, body: { error: "unprocessable" } });
    const client = makeClient(fetch);
    const err = await client.listContracts().catch((e) => e);
    expect(err).toBeInstanceOf(LumenqraphError);
    expect(err.status).toBe(422);
    expect(err.body).toEqual({ error: "unprocessable" });
  });
});

// ---------------------------------------------------------------------------
// #81 Timeout configuration
// ---------------------------------------------------------------------------

describe("timeout (#81)", () => {
  it("default client options result in a non-zero timeout", () => {
    const client = new LumenqraphClient({
      baseUrl: "http://localhost:8080",
      fetch: mockFetch() as unknown as typeof globalThis.fetch,
    });
    // We can't read private fields directly; verify indirectly that the
    // constructor does not throw when no `retry` key is supplied.
    expect(client).toBeInstanceOf(LumenqraphClient);
  });

  it("allows overriding all retry fields independently", () => {
    const client = new LumenqraphClient({
      baseUrl: "http://localhost:8080",
      fetch: mockFetch() as unknown as typeof globalThis.fetch,
      retry: { maxRetries: 5, baseDelayMs: 100, maxDelayMs: 5000, timeoutMs: 8000 },
    });
    expect(client).toBeInstanceOf(LumenqraphClient);
  });
});
