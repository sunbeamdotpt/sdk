import { describe, it } from "@std/testing/bdd";
import { expect } from "@std/expect";
import { spy } from "@std/testing/mock";
import { ServiceError } from "./errors.ts";
import { backoffDelayMs, withRetry } from "./retry.ts";

describe("backoffDelayMs", () => {
  it("respects baseMs * 2^attempt without jitter", () => {
    const p = { maxRetries: 3, baseMs: 100, maxMs: 10_000, jitter: false };
    expect(backoffDelayMs(p, 0)).toBe(100);
    expect(backoffDelayMs(p, 1)).toBe(200);
    expect(backoffDelayMs(p, 2)).toBe(400);
  });

  it("clamps to maxMs", () => {
    const p = { maxRetries: 10, baseMs: 100, maxMs: 500, jitter: false };
    expect(backoffDelayMs(p, 10)).toBe(500);
  });
});

describe("withRetry", () => {
  it("returns first success without retrying", async () => {
    const op = spy(() => Promise.resolve("ok"));
    const result = await withRetry(op, {
      maxRetries: 3,
      baseMs: 1,
      maxMs: 1,
      jitter: false,
    });
    expect(result).toBe("ok");
    expect(op.calls.length).toBe(1);
  });

  it("retries retryable errors up to maxRetries", async () => {
    let callCount = 0;
    const op = spy(() => {
      callCount++;
      if (callCount <= 2) {
        return Promise.reject(new ServiceError("Unavailable", "x"));
      }
      return Promise.resolve("ok");
    });
    const result = await withRetry(op, {
      maxRetries: 3,
      baseMs: 1,
      maxMs: 1,
      jitter: false,
    });
    expect(result).toBe("ok");
    expect(op.calls.length).toBe(3);
  });

  it("does not retry non-retryable errors", async () => {
    const op = spy(() =>
      Promise.reject(new ServiceError("PermissionDenied", "no"))
    );
    await expect(
      withRetry(op, { maxRetries: 5, baseMs: 1, maxMs: 1, jitter: false }),
    ).rejects.toMatchObject({ kind: "PermissionDenied" });
    expect(op.calls.length).toBe(1);
  });

  it("does not retry Unauthenticated by default", async () => {
    const op = spy(() =>
      Promise.reject(new ServiceError("Unauthenticated", "expired"))
    );
    await expect(
      withRetry(op, { maxRetries: 3, baseMs: 1, maxMs: 1, jitter: false }),
    ).rejects.toMatchObject({ kind: "Unauthenticated" });
    expect(op.calls.length).toBe(1);
  });

  it("retries Unauthenticated when retryUnauthenticated is set", async () => {
    let callCount = 0;
    const op = spy(() => {
      callCount++;
      if (callCount === 1) {
        return Promise.reject(new ServiceError("Unauthenticated", "expired"));
      }
      return Promise.resolve("ok");
    });
    const result = await withRetry(op, {
      maxRetries: 2,
      baseMs: 1,
      maxMs: 1,
      jitter: false,
      retryUnauthenticated: true,
    });
    expect(result).toBe("ok");
    expect(op.calls.length).toBe(2);
  });

  it("throws ServiceError after exhausting retries", async () => {
    const op = spy(() =>
      Promise.reject(new ServiceError("Unavailable", "still down"))
    );
    await expect(
      withRetry(op, { maxRetries: 2, baseMs: 1, maxMs: 1, jitter: false }),
    ).rejects.toMatchObject({ kind: "Unavailable" });
    expect(op.calls.length).toBe(3);
  });
});
