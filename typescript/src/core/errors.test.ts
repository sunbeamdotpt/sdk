import { Code, ConnectError } from "@connectrpc/connect";
import { describe, it } from "@std/testing/bdd";
import { expect } from "@std/expect";
import { isRetryable, ServiceError } from "./errors.ts";

describe("ServiceError", () => {
  it("maps Connect codes to kinds", () => {
    const err = ServiceError.fromConnect(new ConnectError("nope", Code.NotFound));
    expect(err.kind).toBe("NotFound");
    expect(err.httpStatus).toBe(404);
    expect(err.code).toBe(Code.NotFound);
  });

  it("treats Unavailable / DeadlineExceeded / Network as retryable", () => {
    expect(new ServiceError("Unavailable", "x").retryable).toBe(true);
    expect(new ServiceError("DeadlineExceeded", "x").retryable).toBe(true);
    expect(new ServiceError("Network", "x").retryable).toBe(true);
    expect(new ServiceError("InvalidArgument", "x").retryable).toBe(false);
    expect(new ServiceError("PermissionDenied", "x").retryable).toBe(false);
  });

  it("ServiceError.from passes through ServiceError unchanged", () => {
    const inner = new ServiceError("NotFound", "x");
    expect(ServiceError.from(inner)).toBe(inner);
  });

  it("ServiceError.from converts AbortError to DeadlineExceeded", () => {
    const ab = new Error("aborted");
    ab.name = "AbortError";
    expect(ServiceError.from(ab).kind).toBe("DeadlineExceeded");
  });

  it("ServiceError.from converts fetch TypeErrors to Network", () => {
    const te = new TypeError("Failed to fetch");
    expect(ServiceError.from(te).kind).toBe("Network");
  });

  it("isRetryable defers to ServiceError.from", () => {
    expect(isRetryable(new ConnectError("x", Code.Unavailable))).toBe(true);
    expect(isRetryable(new ConnectError("x", Code.NotFound))).toBe(false);
  });

  it("isRetryable honors the retryUnauthenticated opt-in", () => {
    const err = new ConnectError("x", Code.Unauthenticated);
    expect(isRetryable(err)).toBe(false);
    expect(isRetryable(err, { retryUnauthenticated: true })).toBe(true);
    expect(isRetryable(new ConnectError("x", Code.NotFound), {
      retryUnauthenticated: true,
    })).toBe(false);
  });
});
