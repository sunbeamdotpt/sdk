import { describe, it } from "@std/testing/bdd";
import { expect } from "@std/expect";
import { spy } from "@std/testing/mock";
import { createRpcLoader, type RpcRouteContext } from "./loaders.ts";
import { ServiceError } from "../core/errors.ts";

const FakeService = {
  typeName: "fake.Service",
  method: {
    get: { kind: "unary" as const },
  },
} as const;

describe("createRpcLoader", () => {
  it("throws ServiceError when transport is missing from context", async () => {
    const loader = createRpcLoader(FakeService as any, "get", { id: "1" });
    const context: RpcRouteContext = {
      queryClient: {} as any,
    };

    await expect(loader({ context })).rejects.toBeInstanceOf(ServiceError);
  });

  it("calls fetchQuery with correct queryKey", async () => {
    const mockFetchQuery = spy((_options: unknown) =>
      Promise.resolve({ id: "1", name: "Alice" })
    );
    const queryClient = { fetchQuery: mockFetchQuery } as any;

    const mockTransport = {} as any;
    const loader = createRpcLoader(FakeService as any, "get", { id: "1" });
    const context: RpcRouteContext = {
      queryClient,
      transport: mockTransport,
    };

    await loader({ context });

    expect(mockFetchQuery.calls[0].args[0]).toEqual(
      expect.objectContaining({
        queryKey: ["fake.Service", "get", { id: "1" }],
      }),
    );
  });
});
