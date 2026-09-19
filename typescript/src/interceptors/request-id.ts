import { type Interceptor } from "@connectrpc/connect";

const HEADER = "x-request-id";

const genId = (): string =>
  globalThis.crypto?.randomUUID?.() ??
  `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;

/**
 * Options for the request ID interceptor.
 * Configures how request IDs are generated and attached to requests.
 */
export interface RequestIdOptions {
  /**
   * Custom ID generator function (default: generates UUID or timestamp-based ID).
   */
  generate?: () => string;

  /**
   * Header name for the request ID (default: "x-request-id").
   */
  headerName?: string;
}

/**
 * Creates a request ID interceptor that attaches unique IDs to outgoing requests.
 *
 * Generates and attaches a request ID to each request for request tracing and correlation.
 * Skips assignment if the header is already set (e.g., by upstream middleware).
 *
 * @param options Configuration for ID generation and header naming.
 * @returns An Interceptor that adds request ID headers to requests.
 *
 * @example
 * const transport = createTransport({
 *   httpClient: fetch,
 *   baseUrl: "http://localhost:8080",
 *   interceptors: [
 *     withRequestId(),
 *   ],
 * });
 */
export function withRequestId(options: RequestIdOptions = {}): Interceptor {
  const headerName = options.headerName ?? HEADER;
  const generate = options.generate ?? genId;
  return (next) => async (req) => {
    if (!req.header.has(headerName)) req.header.set(headerName, generate());
    return next(req);
  };
}
