/**
 * Core transport, error handling, and resilience primitives.
 *
 * Exports Connect RPC transport creation, service error types, retry policies,
 * circuit breaker state management, and health checking utilities. Use these
 * to build resilient gRPC clients with configurable backoff and failure modes.
 *
 * @example
 * ```ts
 * import {
 *   createTransport,
 *   CircuitBreaker,
 *   defaultRetryPolicy,
 * } from "@sunbeam/g2v/core";
 *
 * const transport = createTransport({
 *   baseUrl: "https://api.example.com",
 *   retryPolicy: defaultRetryPolicy,
 * });
 * const breaker = new CircuitBreaker(defaultCircuitConfig);
 * ```
 *
 * @module
 */

export * from "./errors.ts";
export * from "./config.ts";
export * from "./transport.ts";
export * from "./retry.ts";
export * from "./circuit-breaker.ts";
export * from "./health.ts";
