import { ServiceError } from "./errors.ts";

/**
 * Health status of a service: healthy (ok), degraded (ok but slow), or unhealthy (unreachable or error).
 */
export type HealthStatus = "healthy" | "degraded" | "unhealthy";

/**
 * Result of a health check probe.
 */
export interface HealthReport {
  /** The status of the service (healthy, degraded, or unhealthy) */
  status: HealthStatus;
  /** Unix timestamp when the check was performed */
  checkedAt: number;
  /** Response latency in milliseconds; degraded if > 1000ms */
  latencyMs?: number;
  /** Error message if the check failed */
  error?: string;
}

/**
 * Options for probing a service's health.
 */
export interface HealthCheckOptions {
  /** URL of the health check endpoint */
  url: string;
  /** Request timeout in milliseconds (defaults to 5000) */
  timeoutMs?: number;
  /** Custom fetch implementation (defaults to globalThis.fetch) */
  fetchImpl?: typeof fetch;
}

/**
 * Probes a service health endpoint and returns latency and status.
 * Marks as degraded if latency exceeds 1 second, unhealthy on non-ok responses or timeouts.
 * @param options Health check configuration
 * @returns A HealthReport with status, latency, and optional error message
 * @example
 * ```ts
 * const health = await probeHealth({ url: "https://api.example.com/health" });
 * if (health.status === "unhealthy") {
 *   console.error("Service down:", health.error);
 * }
 * ```
 */
export async function probeHealth(
  options: HealthCheckOptions,
): Promise<HealthReport> {
  const fetchImpl = options.fetchImpl ?? fetch;
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(),
    options.timeoutMs ?? 5_000,
  );
  const start = performance.now();
  try {
    const res = await fetchImpl(options.url, {
      method: "GET",
      signal: controller.signal,
    });
    const latencyMs = performance.now() - start;
    if (!res.ok) {
      return {
        status: "unhealthy",
        checkedAt: Date.now(),
        latencyMs,
        error: `HTTP ${res.status}`,
      };
    }
    return {
      status: latencyMs > 1_000 ? "degraded" : "healthy",
      checkedAt: Date.now(),
      latencyMs,
    };
  } catch (err) {
    return {
      status: "unhealthy",
      checkedAt: Date.now(),
      error: ServiceError.from(err).message,
    };
  } finally {
    clearTimeout(timeout);
  }
}
