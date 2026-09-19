import type { Interceptor } from "@connectrpc/connect";
import { ServiceError } from "../core/errors.ts";
import { backoffDelayMs, type RetryPolicy } from "../core/retry.ts";

/**
 * Backoff policy for the single refresh-and-retry attempt after a 401.
 * Kept deliberately small: this delay only gives the refreshed token time to
 * propagate before the one retry.
 */
const REFRESH_RETRY_POLICY: RetryPolicy = {
  maxRetries: 1,
  baseMs: 100,
  maxMs: 500,
  jitter: false,
};

/** Delays execution for the given number of milliseconds. */
const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Options for the authentication interceptor.
 * Configures how bearer tokens are obtained and attached to requests.
 */
export interface AuthInterceptorOptions {
  /**
   * Function that returns a token string (or undefined if unavailable).
   * May be async to support dynamic token fetching (e.g., from a session store).
   */
  getToken: () => string | undefined | Promise<string | undefined>;

  /**
   * Authentication scheme prefix (default: "Bearer").
   */
  scheme?: string;

  /**
   * Header name for the token (default: "authorization").
   */
  headerName?: string;

  /**
   * Optional refresh hook invoked when a call fails with Unauthenticated.
   * Must invalidate the rejected token and return a fresh token (or null if
   * none could be obtained). When configured, the call is retried exactly
   * once with the fresh token after a small backoff.
   */
  refresh?: () => string | null | Promise<string | null>;
}

/**
 * Creates an authentication interceptor that adds bearer tokens to requests.
 *
 * Attaches a token (from the provided getter) to request headers using the configured
 * scheme and header name. Skips token attachment if the token is unavailable.
 *
 * When a unary call fails with Unauthenticated and a `refresh` hook is configured,
 * the hook is invoked to obtain a fresh token and the call is retried exactly once
 * after a small backoff. Without a `refresh` hook, an Unauthenticated failure is
 * re-thrown as a ServiceError explaining that the token is not refreshable.
 * Streaming calls are never retried (their request body may be consumed).
 *
 * @param options Configuration for token fetching and header naming.
 * @returns An Interceptor that adds authentication headers to requests.
 *
 * @example
 * const transport = createTransport({
 *   httpClient: fetch,
 *   baseUrl: "http://localhost:8080",
 *   interceptors: [
 *     withAuth({
 *       getToken: () => sessionStore.accessToken,
 *       refresh: () => sessionStore.refreshAccessToken(),
 *     }),
 *   ],
 * });
 */
export function withAuth(options: AuthInterceptorOptions): Interceptor {
  const scheme = options.scheme ?? "Bearer";
  const headerName = options.headerName ?? "authorization";
  return (next) => async (req) => {
    const token = await options.getToken();
    if (token) req.header.set(headerName, `${scheme} ${token}`);
    try {
      return await next(req);
    } catch (err) {
      const svc = ServiceError.from(err);
      if (svc.kind !== "Unauthenticated" || req.stream) throw err;
      if (!options.refresh) {
        throw new ServiceError(
          "Unauthenticated",
          "Request was rejected as unauthenticated and the configured token " +
            "is not refreshable; provide a `refresh` hook to withAuth to " +
            "enable automatic token renewal.",
          err,
        );
      }
      const fresh = await options.refresh();
      if (!fresh) {
        throw new ServiceError(
          "Unauthenticated",
          "Request was rejected as unauthenticated and the `refresh` hook " +
            "did not return a fresh token.",
          err,
        );
      }
      await sleep(backoffDelayMs(REFRESH_RETRY_POLICY, 0));
      req.header.set(headerName, `${scheme} ${fresh}`);
      return next(req);
    }
  };
}
