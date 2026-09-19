import { isRetryable, ServiceError } from "./errors.ts";

/**
 * Policy for retrying failed operations. Specifies max attempts, backoff strategy, and retry predicates.
 */
export interface RetryPolicy {
  /** Maximum number of retry attempts after the initial failure */
  maxRetries: number;
  /** Initial backoff delay in milliseconds */
  baseMs: number;
  /** Maximum backoff delay in milliseconds */
  maxMs: number;
  /** Whether to add randomness to backoff delays (true by default) */
  jitter?: boolean;
  /**
   * Whether Unauthenticated errors should be retried (false by default).
   * Only enable when a token refresh mechanism is in place (e.g. the
   * `refresh` hook of `withAuth`), otherwise retries re-serve the same
   * rejected token.
   */
  retryUnauthenticated?: boolean;
  /** Optional custom predicate to determine if an error should be retried */
  shouldRetry?: (err: ServiceError, attempt: number) => boolean;
}

/**
 * Default retry policy: 3 attempts, 100–5000ms exponential backoff with jitter.
 */
export const defaultRetryPolicy: RetryPolicy = {
  maxRetries: 3,
  baseMs: 100,
  maxMs: 5_000,
  jitter: true,
};

/**
 * Computes the backoff delay for a given retry attempt using exponential backoff.
 * @param policy The retry policy configuration
 * @param attempt Zero-indexed attempt number
 * @returns Milliseconds to wait before the next attempt (may include jitter)
 */
export function backoffDelayMs(policy: RetryPolicy, attempt: number): number {
  const exp = Math.min(policy.maxMs, policy.baseMs * 2 ** attempt);
  if (policy.jitter === false) return exp;
  return Math.floor(Math.random() * exp);
}

/**
 * Delays execution with optional abort support.
 */
const sleep = (ms: number, signal?: AbortSignal): Promise<void> =>
  new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(signal.reason);
    const id = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(id);
        reject(signal.reason);
      },
      { once: true },
    );
  });

/**
 * Retries an operation using exponential backoff. Automatically retries on retryable errors
 * (Unavailable, Network, DeadlineExceeded — plus Unauthenticated when the policy sets
 * `retryUnauthenticated`) unless a custom shouldRetry predicate is provided.
 * @param op Async operation that receives the attempt number (0-indexed)
 * @param policy Retry configuration; defaults to defaultRetryPolicy
 * @param signal Optional AbortSignal to cancel retries early
 * @returns The result of the operation
 * @throws ServiceError if all attempts fail or the operation fails with a non-retryable error
 * @example
 * ```ts
 * const result = await withRetry(
 *   (attempt) => fetchData(),
 *   { maxRetries: 5, baseMs: 200, maxMs: 10_000 }
 * );
 * ```
 */
export async function withRetry<T>(
  op: (attempt: number) => Promise<T>,
  policy: RetryPolicy = defaultRetryPolicy,
  signal?: AbortSignal,
): Promise<T> {
  let lastErr: unknown;
  for (let attempt = 0; attempt <= policy.maxRetries; attempt++) {
    try {
      return await op(attempt);
    } catch (err) {
      lastErr = err;
      const svcErr = ServiceError.from(err);
      const retryable = policy.shouldRetry
        ? policy.shouldRetry(svcErr, attempt)
        : isRetryable(svcErr, {
          retryUnauthenticated: policy.retryUnauthenticated,
        });
      if (!retryable || attempt === policy.maxRetries) throw svcErr;
      await sleep(backoffDelayMs(policy, attempt), signal);
    }
  }
  throw ServiceError.from(lastErr);
}
