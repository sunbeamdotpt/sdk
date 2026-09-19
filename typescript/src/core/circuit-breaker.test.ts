import { describe, it } from "@std/testing/bdd";
import { expect } from "@std/expect";
import { spy } from "@std/testing/mock";
import { CircuitBreaker } from "./circuit-breaker.ts";
import { ServiceError } from "./errors.ts";

describe("CircuitBreaker", () => {
  it("stays closed under successes", async () => {
    const cb = new CircuitBreaker({ failureThreshold: 3, resetMs: 1_000 });
    for (let i = 0; i < 5; i++) {
      expect(await cb.run(async () => 42)).toBe(42);
    }
    expect(cb.currentState).toBe("closed");
  });

  it("opens after failureThreshold consecutive failures", async () => {
    const cb = new CircuitBreaker({ failureThreshold: 2, resetMs: 1_000 });
    const op = () => Promise.reject(new Error("boom"));
    await expect(cb.run(op)).rejects.toThrow();
    await expect(cb.run(op)).rejects.toThrow();
    expect(cb.currentState).toBe("open");
    await expect(cb.run(op)).rejects.toMatchObject({
      message: expect.stringMatching(/circuit breaker open/),
    });
  });

  it("transitions to half-open after resetMs and closes on probe success", async () => {
    let now = 0;
    const clock = spy(() => now);
    const cb = new CircuitBreaker({
      failureThreshold: 1,
      resetMs: 100,
      now: clock,
    });
    await expect(cb.run(() => Promise.reject(new Error("boom")))).rejects.toThrow();
    expect(cb.currentState).toBe("open");
    now = 200;
    expect(cb.currentState).toBe("half-open");
    expect(await cb.run(async () => "ok")).toBe("ok");
    expect(cb.currentState).toBe("closed");
  });

  it("rejects extra half-open probes beyond halfOpenMaxAttempts", async () => {
    let now = 0;
    const cb = new CircuitBreaker({
      failureThreshold: 1,
      resetMs: 50,
      halfOpenMaxAttempts: 1,
      now: () => now,
    });
    await expect(cb.run(() => Promise.reject(new Error("x")))).rejects.toThrow();
    now = 100;
    const slow = new Promise<string>((r) => setTimeout(() => r("ok"), 5));
    const probe = cb.run(() => slow);
    await expect(cb.run(async () => "second")).rejects.toMatchObject({
      message: expect.stringMatching(/half-open/),
    });
    await probe;
  });

  it("ServiceError thrown when open is Unavailable", async () => {
    const cb = new CircuitBreaker({ failureThreshold: 1, resetMs: 10_000 });
    await expect(cb.run(() => Promise.reject(new Error("x")))).rejects.toThrow();
    try {
      await cb.run(async () => "x");
    } catch (e) {
      expect(e).toBeInstanceOf(ServiceError);
      expect((e as ServiceError).kind).toBe("Unavailable");
    }
  });
});
