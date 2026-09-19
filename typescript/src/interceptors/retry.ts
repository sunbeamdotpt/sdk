import { type Interceptor } from "@connectrpc/connect";
import {
  defaultRetryPolicy,
  type RetryPolicy,
  withRetry,
} from "../core/retry.ts";

/**
 * Options for the retry interceptor.
 * Configures retry policy and whether to apply only to idempotent methods.
 */
export interface RetryInterceptorOptions {
  /**
   * Retry policy defining backoff, max attempts, and retryable error codes (default: defaultRetryPolicy).
   */
  policy?: RetryPolicy;

  /**
   * If true, only apply retries to idempotent methods (default: false, retries all non-streaming RPCs).
   */
  onlyIdempotent?: boolean;
}

/**
 * Creates a retry interceptor that automatically retries failed RPC calls.
 *
 * Applies exponential backoff retries to non-streaming unary calls according to the specified policy.
 * Optionally restricts retries to idempotent methods only. Streaming calls are never retried.
 *
 * @param options Configuration for retry policy and idempotency checks.
 * @returns An Interceptor that retries eligible RPC calls on failure.
 *
 * @example
 * const transport = createTransport({
 *   httpClient: fetch,
 *   baseUrl: "http://localhost:8080",
 *   interceptors: [
 *     withRetryInterceptor({ onlyIdempotent: true }),
 *   ],
 * });
 */
export function withRetryInterceptor(
  options: RetryInterceptorOptions = {},
): Interceptor {
  const policy = options.policy ?? defaultRetryPolicy;
  return (next) => (req) => {
    if (req.stream) return next(req);
    if (options.onlyIdempotent && !req.method.idempotency) return next(req);
    return withRetry(() => next(req), policy);
  };
}
