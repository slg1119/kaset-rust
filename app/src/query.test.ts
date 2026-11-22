import { expect, test } from "vitest";
import { createQueryClient, shouldRetry } from "./query";

test("retries network and HTTP failures once", () => {
  expect(shouldRetry(0, { kind: "network", message: "offline" })).toBe(true);
  expect(shouldRetry(0, { kind: "http", message: "HTTP 500" })).toBe(true);
  expect(shouldRetry(1, { kind: "network", message: "offline" })).toBe(false);
});

test("never retries errors a retry cannot fix", () => {
  for (const kind of ["auth_expired", "not_signed_in", "invalid_argument", "parse", "rate_limited", "internal"]) {
    expect(shouldRetry(0, { kind, message: kind })).toBe(false);
  }
  expect(shouldRetry(0, new Error("boom"))).toBe(false);
});

test("the client does not refetch on window focus", () => {
  const client = createQueryClient();
  expect(client.getDefaultOptions().queries?.refetchOnWindowFocus).toBe(false);
  expect(createQueryClient({ retry: false }).getDefaultOptions().queries?.retry).toBe(false);
});
