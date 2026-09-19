import type { Interceptor, Transport } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { withRequestId } from "../interceptors/request-id.ts";
import { withTracing } from "../interceptors/tracing.ts";
import type { TransportConfig } from "./config.ts";

/**
 * Options for creating a Connect transport. Extends TransportConfig with optional interceptors and fetch override.
 */
export interface CreateTransportOptions extends TransportConfig {
  /**
   * List of Connect interceptors to attach to the transport.
   * When omitted, defaults to `[withRequestId(), withTracing()]` so every
   * request carries an `x-request-id` and W3C trace context.
   */
  interceptors?: Interceptor[];
  /** Custom fetch implementation (defaults to globalThis.fetch) */
  fetch?: typeof fetch;
}

/**
 * Creates a Connect RPC transport for use with generated service clients.
 * Configures credentials, timeout, and binary format preferences.
 *
 * When `options.interceptors` is omitted, the default interceptors
 * `[withRequestId(), withTracing()]` are applied; a caller-supplied array is
 * used as-is (no defaults are merged in).
 * @param options Transport configuration and optional interceptors
 * @returns A configured Connect Transport ready to use with service clients
 * @example
 * ```ts
 * const transport = createTransport({
 *   baseUrl: "https://api.example.com",
 *   credentials: "include",
 *   defaultTimeoutMs: 10_000
 * });
 * ```
 */
export function createTransport(options: CreateTransportOptions): Transport {
  const credentials = options.credentials ?? "same-origin";
  const baseFetch = options.fetch ?? globalThis.fetch.bind(globalThis);
  const fetchWithCredentials: typeof fetch = (input, init) =>
    baseFetch(input, { credentials, ...init });

  return createConnectTransport({
    baseUrl: options.baseUrl,
    useBinaryFormat: options.useBinaryFormat ?? false,
    interceptors: options.interceptors ?? [withRequestId(), withTracing()],
    fetch: fetchWithCredentials,
    defaultTimeoutMs: options.defaultTimeoutMs ?? 30_000,
  });
}
