/**
 * Browser OpenTelemetry setup and context management.
 *
 * Exports utilities to initialize and shut down observability pipelines in
 * browser environments: tracing exporters, metrics collectors, and active span
 * context for automatic request tracing across async operations.
 *
 * @example
 * ```tsx
 * import { setupOtel, shutdownOtel } from "@sunbeam/g2v/otel";
 *
 * await setupOtel({ serviceName: "my-app", exporter: "otlp" });
 * // ... app runs ...
 * await shutdownOtel();
 * ```
 *
 * @module
 */

export * from "./setup.ts";
