import type { Interceptor } from "@connectrpc/connect";
import {
  context,
  propagation,
  SpanKind,
  SpanStatusCode,
  trace,
  type Tracer,
} from "@opentelemetry/api";
import { ServiceError } from "../core/errors.ts";

/** Header used to correlate client spans with server logs. */
const REQUEST_ID_HEADER = "x-request-id";

/**
 * Options for the OpenTelemetry tracing interceptor.
 * Configures the tracer instance and naming for spans.
 */
export interface TracingOptions {
  /**
   * Tracer name for span creation (default: "sunbeam-g2v").
   * Used when no explicit tracer instance is provided.
   */
  tracerName?: string;

  /**
   * OpenTelemetry Tracer instance (default: obtained from global tracer provider).
   */
  tracer?: Tracer;
}

/**
 * Creates an OpenTelemetry tracing interceptor for RPC calls.
 *
 * Creates a span per RPC call with service/method attributes, injects trace context into headers
 * for propagation, and records span status and errors. Follows OpenTelemetry semantic conventions.
 * The `x-request-id` header (set by an earlier request-ID interceptor, or echoed back by the
 * server) is recorded as the `request_id` span attribute for log correlation.
 *
 * @param options Configuration for the tracer instance and naming.
 * @returns An Interceptor that creates and records distributed traces for RPC calls.
 *
 * @example
 * const transport = createTransport({
 *   httpClient: fetch,
 *   baseUrl: "http://localhost:8080",
 *   interceptors: [
 *     withTracing({ tracerName: "my-service" }),
 *   ],
 * });
 */
export function withTracing(options: TracingOptions = {}): Interceptor {
  const tracerName = options.tracerName ?? "sunbeam-g2v";
  const tracer = options.tracer ?? trace.getTracer(tracerName);
  return (next) => (req) => {
    const spanName = `${req.service.typeName}/${req.method.name}`;
    return tracer.startActiveSpan(
      spanName,
      {
        kind: SpanKind.CLIENT,
        attributes: {
          "rpc.system": "connectrpc",
          "rpc.service": req.service.typeName,
          "rpc.method": req.method.name,
        },
      },
      async (span) => {
        const headers: Record<string, string> = {};
        propagation.inject(context.active(), headers);
        for (const [k, v] of Object.entries(headers)) req.header.set(k, v);
        // Correlate the span with the request ID attached earlier in the
        // interceptor chain (e.g. by withRequestId).
        const requestId = req.header.get(REQUEST_ID_HEADER);
        if (requestId) span.setAttribute("request_id", requestId);
        try {
          const res = await next(req);
          // Prefer the server-echoed request ID when the request had none.
          if (!requestId) {
            const echoed = res.header.get(REQUEST_ID_HEADER);
            if (echoed) span.setAttribute("request_id", echoed);
          }
          span.setStatus({ code: SpanStatusCode.OK });
          return res;
        } catch (err) {
          const svc = ServiceError.from(err);
          span.setStatus({
            code: SpanStatusCode.ERROR,
            message: svc.message,
          });
          span.setAttribute("rpc.connect.error_code", svc.code);
          span.recordException(svc);
          throw err;
        } finally {
          span.end();
        }
      },
    );
  };
}
