import { describe, it } from "@std/testing/bdd";
import { expect } from "@std/expect";
import {
  Code,
  ConnectError,
  type StreamRequest,
  type UnaryRequest,
  type UnaryResponse,
} from "@connectrpc/connect";
import { withAuth } from "./auth.ts";

function fakeReq(): UnaryRequest {
  return {
    stream: false,
    service: { typeName: "test.EchoService" },
    method: { name: "Say" },
    header: new Headers(),
    message: {},
  } as unknown as UnaryRequest;
}

function fakeRes(req: UnaryRequest | StreamRequest): UnaryResponse {
  return {
    stream: false,
    service: req.service,
    method: req.method,
    header: new Headers(),
    message: {},
  } as unknown as UnaryResponse;
}

describe("withAuth", () => {
  it("attaches the token as a Bearer authorization header", async () => {
    const interceptor = withAuth({ getToken: () => "token-1" });
    const req = fakeReq();
    await interceptor((r) => Promise.resolve(fakeRes(r)))(req);
    expect(req.header.get("authorization")).toBe("Bearer token-1");
  });

  it("refreshes the token and retries once after a 401", async () => {
    let calls = 0;
    const tokens: (string | null)[] = [];
    const interceptor = withAuth({
      getToken: () => "old-token",
      refresh: () => Promise.resolve("new-token"),
    });
    const req = fakeReq();
    const res = await interceptor((r) => {
      calls++;
      tokens.push(r.header.get("authorization"));
      if (calls === 1) {
        return Promise.reject(new ConnectError("expired", Code.Unauthenticated));
      }
      return Promise.resolve(fakeRes(r));
    })(req);
    expect(calls).toBe(2);
    expect(tokens).toEqual(["Bearer old-token", "Bearer new-token"]);
    expect(res.message).toEqual({});
  });

  it("fails fast with a clear error when no refresh hook is configured", async () => {
    let calls = 0;
    const interceptor = withAuth({ getToken: () => "static-token" });
    const req = fakeReq();
    await expect(
      interceptor((r) => {
        calls++;
        return Promise.reject(
          new ConnectError("expired", Code.Unauthenticated),
        );
      })(req),
    ).rejects.toThrow(/not refreshable/);
    expect(calls).toBe(1);
  });

  it("fails with a clear error when the refresh hook returns no token", async () => {
    let calls = 0;
    const interceptor = withAuth({
      getToken: () => "old-token",
      refresh: () => null,
    });
    const req = fakeReq();
    await expect(
      interceptor((r) => {
        calls++;
        return Promise.reject(
          new ConnectError("expired", Code.Unauthenticated),
        );
      })(req),
    ).rejects.toThrow(/did not return a fresh token/);
    expect(calls).toBe(1);
  });

  it("rethrows non-authentication errors unchanged", async () => {
    let calls = 0;
    const interceptor = withAuth({
      getToken: () => "token-1",
      refresh: () => "new-token",
    });
    const req = fakeReq();
    const err = new ConnectError("missing", Code.NotFound);
    await expect(
      interceptor((r) => {
        calls++;
        return Promise.reject(err);
      })(req),
    ).rejects.toBe(err);
    expect(calls).toBe(1);
  });
});
