import { describe, it, beforeEach } from "@std/testing/bdd";
import { expect } from "@std/expect";
import { spy } from "@std/testing/mock";
import { createRestClient } from "./client.ts";
import { authActions } from "../state/auth.ts";

describe("createRestClient", () => {
  beforeEach(() => {
    authActions.logout();
  });

  it("performs a GET request and returns JSON", async () => {
    const fetchMock = spy((_url: string, _init: RequestInit) =>
      Promise.resolve({
        ok: true,
        status: 200,
        headers: new Headers({ "content-type": "application/json" }),
        json: async () => ({ id: 1, name: "Alice" }),
        text: async () => "",
      } as Response)
    );

    const client = createRestClient({
      baseUrl: "https://api.example.com",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const result = await client.get("/users/1");
    expect(result).toEqual({ id: 1, name: "Alice" });
    expect(fetchMock.calls[0].args[0]).toBe("https://api.example.com/users/1");
    expect(fetchMock.calls[0].args[1]).toEqual(
      expect.objectContaining({ method: "GET" }),
    );
  });

  it("injects auth token when authenticated", async () => {
    authActions.loginSuccess({
      accessToken: "secret-token",
      claims: { sub: "user-1" },
    });

    const fetchMock = spy((_url: string, _init: RequestInit) =>
      Promise.resolve({
        ok: true,
        status: 200,
        headers: new Headers({ "content-type": "application/json" }),
        json: async () => ({}),
        text: async () => "",
      } as Response)
    );

    const client = createRestClient({
      baseUrl: "https://api.example.com",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await client.get("/profile");
    const init = fetchMock.calls[0].args[1] as RequestInit;
    expect((init.headers as Record<string, string>)["Authorization"]).toBe(
      "Bearer secret-token",
    );
  });

  it("throws ServiceError on non-ok response", async () => {
    const fetchMock = spy((_url: string, _init: RequestInit) =>
      Promise.resolve({
        ok: false,
        status: 404,
        headers: new Headers(),
        json: async () => ({}),
        text: async () => "Not found",
      } as Response)
    );

    const client = createRestClient({
      baseUrl: "https://api.example.com",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(client.get("/missing")).rejects.toThrow("Not found");
  });

  it("throws ServiceError on network failure", async () => {
    const fetchMock = spy((_url: string, _init: RequestInit) =>
      Promise.reject(new TypeError("fetch failed"))
    );

    const client = createRestClient({
      baseUrl: "https://api.example.com",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await expect(client.get("/users")).rejects.toThrow("fetch failed");
  });
});
