/**
 * Connect RPC interceptors for authentication, logging, tracing, and resilience.
 *
 * Exports composable interceptor factories for request/response lifecycle hooks:
 * auth (bearer token injection), request-id (correlation tracking), logging
 * (structured output), tracing (OpenTelemetry spans), retry (exponential backoff),
 * and circuit breaker (failure isolation).
 *
 * @example
 * ```ts
 * import { withAuth, withLogging, withTracing } from "@sunbeam/g2v/interceptors";
 *
 * const interceptors = [
 *   withAuth({ token: "bearer ..." }),
 *   withLogging({ logger: console }),
 *   withTracing({}),
 * ];
 * ```
 *
 * @module
 */

export * from "./auth.ts";
export * from "./request-id.ts";
export * from "./logging.ts";
export * from "./tracing.ts";
export * from "./retry.ts";
export * from "./circuit.ts";
