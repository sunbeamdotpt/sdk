import { useQuery } from "@tanstack/react-query";
import { probeHealth, type HealthCheckOptions, type HealthReport } from "../core/health.ts";

/**
 * Configuration options for useHealth hook.
 * Extends HealthCheckOptions with polling and enable/disable controls.
 * @property intervalMs - Optional polling interval in milliseconds. Defaults to 30000 (30 seconds).
 * @property enabled - Optional flag to enable or disable health checks. Defaults to true.
 */
export interface UseHealthOptions extends HealthCheckOptions {
  /** Polling interval in milliseconds (defaults to a sensible value when omitted). */
  intervalMs?: number;
  /** When false, suspends polling. */
  enabled?: boolean;
}

/**
 * Hook to periodically probe the health of a service.
 * Automatically refetches at the specified interval and provides the health report.
 *
 * @param options - Configuration including the URL to probe, polling interval, and enabled flag.
 * @returns Object containing:
 *   - report: The HealthReport if a successful check has been performed, undefined otherwise.
 *   - isHealthy: Boolean convenience flag, true if report?.status === "healthy".
 *   - refetch: Function to manually trigger a health check.
 *
 * @example
 * ```tsx
 * const { isHealthy, report, refetch } = useHealth({
 *   url: "https://api.example.com/health",
 *   intervalMs: 10_000,
 * });
 *
 * return (
 *   <div>
 *     Status: {isHealthy ? "healthy" : "unhealthy"}
 *     <button onClick={() => refetch()}>Check now</button>
 *   </div>
 * );
 * ```
 */
export function useHealth(options: UseHealthOptions): {
  report: HealthReport | undefined;
  isHealthy: boolean;
  refetch: () => void;
} {
  const query = useQuery({
    queryKey: ["sunbeam-g2v", "health", options.url],
    queryFn: () => probeHealth(options),
    refetchInterval: options.intervalMs ?? 30_000,
    enabled: options.enabled ?? true,
  });
  return {
    report: query.data,
    isHealthy: query.data?.status === "healthy",
    refetch: () => {
      query.refetch();
    },
  };
}
