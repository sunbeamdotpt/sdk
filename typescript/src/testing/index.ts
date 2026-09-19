/**
 * Testing utilities for unit testing gRPC clients without a live server.
 *
 * Exports createMockTransport to build in-memory Connect RPC transports with
 * mock service handlers. Useful for testing hooks and components that depend
 * on RPC calls without integration test overhead.
 *
 * @example
 * ```ts
 * import { createMockTransport } from "@sunbeam/g2v/testing";
 *
 * const transport = createMockTransport({
 *   routes: (router) => {
 *     router.rpc(MyService, MyService.GetUser, async (req) => ({
 *       id: "123",
 *       name: "Test User",
 *     }));
 *   },
 * });
 * ```
 *
 * @module
 */

import { type Interceptor, type Transport } from "@connectrpc/connect";
import { createRouterTransport } from "@connectrpc/connect";

/**
 * Configuration options for creating a mock Connect RPC transport.
 * @property routes - Callback function that registers mock service routes. The callback receives a router builder.
 * @property interceptors - Optional list of Connect RPC interceptors to apply to the mock transport.
 */
export interface MockTransportOptions {
  /** Callback that registers mock RPC handlers on the supplied router builder. */
  routes: (router: Parameters<Parameters<typeof createRouterTransport>[0]>[0]) => void;
  /** Optional Connect interceptors applied to the mock transport (for round-tripping auth/tracing in tests). */
  interceptors?: Interceptor[];
}

/**
 * Create a mock Connect RPC transport for testing.
 * Useful for unit tests and component tests where you want to avoid network calls
 * and instead provide deterministic mock responses.
 *
 * @param options - Configuration containing route definitions and optional interceptors.
 * @returns A Connect RPC Transport instance that uses in-memory routing.
 *
 * @example
 * ```ts
 * const transport = createMockTransport({
 *   routes: (router) => {
 *     router.rpc(MyService, MyService.GetUser, async (req) => ({
 *       id: "123",
 *       name: "Test User",
 *     }));
 *   },
 * });
 *
 * const client = createClient(MyService, transport);
 * const user = await client.getUser({ id: "123" });
 * ```
 */
export function createMockTransport(options: MockTransportOptions): Transport {
  return createRouterTransport(options.routes, {
    transport: { interceptors: options.interceptors },
  });
}
