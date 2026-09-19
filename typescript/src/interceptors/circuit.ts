import { type Interceptor } from "@connectrpc/connect";
import { CircuitBreaker, type CircuitBreakerConfig } from "../core/circuit-breaker.ts";

/**
 * Options for the circuit breaker interceptor.
 * Configures circuit breaker behavior or provides a pre-configured instance.
 */
export interface CircuitInterceptorOptions {
  /**
   * Circuit breaker configuration (default: CircuitBreaker defaults).
   * Ignored if a breaker instance is explicitly provided.
   */
  config?: CircuitBreakerConfig;

  /**
   * Pre-configured CircuitBreaker instance (default: new instance created from config).
   */
  breaker?: CircuitBreaker;
}

/**
 * Creates a circuit breaker interceptor that prevents cascading failures.
 *
 * Protects against repeated failures by rapidly failing requests when a service is degraded.
 * Maintains circuit state across all requests using a shared breaker instance.
 * Streaming calls are never passed through the circuit breaker.
 *
 * @param options Configuration for the circuit breaker instance.
 * @returns An Interceptor that enforces circuit breaking on unary RPC calls.
 *
 * @example
 * const transport = createTransport({
 *   httpClient: fetch,
 *   baseUrl: "http://localhost:8080",
 *   interceptors: [
 *     withCircuit({ config: { failureThreshold: 5 } }),
 *   ],
 * });
 */
export function withCircuit(
  options: CircuitInterceptorOptions = {},
): Interceptor {
  const breaker = options.breaker ?? new CircuitBreaker(options.config);
  return (next) => (req) => {
    if (req.stream) return next(req);
    return breaker.run(() => next(req));
  };
}
