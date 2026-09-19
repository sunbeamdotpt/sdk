import { context, trace, type Tracer } from "@opentelemetry/api";
import { ZoneContextManager } from "@opentelemetry/context-zone";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-http";
import { registerInstrumentations } from "@opentelemetry/instrumentation";
import { FetchInstrumentation } from "@opentelemetry/instrumentation-fetch";
import { UserInteractionInstrumentation } from "@opentelemetry/instrumentation-user-interaction";
import { XMLHttpRequestInstrumentation } from "@opentelemetry/instrumentation-xml-http-request";
import { Resource } from "@opentelemetry/resources";
import {
  BatchSpanProcessor,
  TraceIdRatioBasedSampler,
  WebTracerProvider,
} from "@opentelemetry/sdk-trace-web";
import {
  ATTR_SERVICE_NAME,
  ATTR_SERVICE_VERSION,
} from "@opentelemetry/semantic-conventions";

/**
 * Configuration options for OpenTelemetry setup.
 * @property serviceName - The name of the service to report traces for.
 * @property serviceVersion - Optional version of the service.
 * @property environment - Optional deployment environment (e.g., "production", "staging").
 * @property otlpUrl - Optional URL for the OTLP trace exporter. If omitted, traces are not exported.
 * @property sampleRatio - Optional sampling ratio between 0 and 1. Defaults to 1.0 (100% sampling).
 * @property propagateTraceHeaderCorsUrls - Optional list of URL patterns to propagate trace headers to (CORS-safe).
 * @property ignoreUrls - Optional list of URL patterns to ignore when creating traces.
 * @property attributes - Optional additional resource attributes to attach to all spans.
 */
export interface OtelSetupOptions {
  /** Service name reported as the OTel resource attribute (e.g., "source-ui"). */
  serviceName: string;
  /** Version string reported alongside the service name. */
  serviceVersion?: string;
  /** Deployment environment ("production", "staging", etc.). */
  environment?: string;
  /** OTLP/HTTP endpoint that traces are exported to. */
  otlpUrl?: string;
  /** Fraction of root spans that are sampled (0–1). */
  sampleRatio?: number;
  /** Origins that may receive trace context headers (CORS allowlist). */
  propagateTraceHeaderCorsUrls?: RegExp[] | string[];
  /** Fetch/XHR URL patterns excluded from auto-instrumentation. */
  ignoreUrls?: RegExp[] | string[];
  /** Extra resource attributes attached to every span. */
  attributes?: Record<string, string>;
}

let provider: WebTracerProvider | undefined;

/**
 * Initialize and configure OpenTelemetry tracing for the application.
 * Registers instrumentations for fetch, XMLHttpRequest, and user interactions.
 * If already initialized, returns the existing tracer for the service.
 *
 * @param options - Configuration options for OpenTelemetry setup.
 * @returns A Tracer instance for recording spans.
 *
 * @example
 * ```ts
 * const tracer = setupOtel({
 *   serviceName: "my-app",
 *   environment: "production",
 *   otlpUrl: "https://otel-collector.example.com",
 *   sampleRatio: 0.1,
 * });
 * ```
 */
export function setupOtel(options: OtelSetupOptions): Tracer {
  if (provider) return trace.getTracer(options.serviceName);

  const resource = new Resource({
    [ATTR_SERVICE_NAME]: options.serviceName,
    ...(options.serviceVersion && {
      [ATTR_SERVICE_VERSION]: options.serviceVersion,
    }),
    ...(options.environment && { "deployment.environment": options.environment }),
    ...options.attributes,
  });

  const exporters = options.otlpUrl
    ? [new BatchSpanProcessor(new OTLPTraceExporter({ url: options.otlpUrl }))]
    : [];

  provider = new WebTracerProvider({
    resource,
    sampler: new TraceIdRatioBasedSampler(options.sampleRatio ?? 1.0),
    spanProcessors: exporters,
  });

  provider.register({ contextManager: new ZoneContextManager() });

  registerInstrumentations({
    instrumentations: [
      new FetchInstrumentation({
        propagateTraceHeaderCorsUrls: options.propagateTraceHeaderCorsUrls,
        ignoreUrls: options.ignoreUrls,
      }),
      new XMLHttpRequestInstrumentation({
        propagateTraceHeaderCorsUrls: options.propagateTraceHeaderCorsUrls,
        ignoreUrls: options.ignoreUrls,
      }),
      new UserInteractionInstrumentation({
        eventNames: ["click", "submit"],
      }),
    ],
  });

  return trace.getTracer(options.serviceName);
}

/**
 * Gracefully shut down the OpenTelemetry provider and flush pending spans.
 *
 * @returns A promise that resolves when shutdown is complete.
 */
export async function shutdownOtel(): Promise<void> {
  if (!provider) return;
  await provider.shutdown();
  provider = undefined;
}

/**
 * Get the currently active OpenTelemetry context.
 *
 * @returns The active context for span association.
 */
export function activeContext(): ReturnType<typeof context.active> {
  return context.active();
}
