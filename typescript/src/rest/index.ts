/**
 * REST data fetching for the Sunbeam G2V framework.
 *
 * Provides TanStack Query hooks and a typed REST client for plain HTTP
 * endpoints that don't speak ConnectRPC/gRPC-web. Reuses the framework's
 * auth token injection, {@link ServiceError} normalization, and retry logic.
 *
 * @example
 * ```tsx
 * import { createRestClient, useRestQuery, useRestMutation } from "@sunbeam/g2v/rest";
 *
 * const api = createRestClient({ baseUrl: "https://api.example.com" });
 * const { data } = useRestQuery(api, "/users/123");
 * const update = useRestMutation(api, "PUT", "/users/123");
 * ```
 *
 * @module
 */

export * from "./client.ts";
export * from "./hooks.ts";
