import { describe, it } from "@std/testing/bdd";
import { expect } from "@std/expect";
import type { UnaryRequest, UnaryResponse } from "@connectrpc/connect";
import {
  propagation,
  type Span,
  type TextMapPropagator,
  type Tracer,
} from "@opentelemetry/api";
import { withTracing } from "./tracing.ts";

/** Fake tracer that records span attributes set during the call. */
function fakeTracer() {
  const attributes: Record<string, unknown> = {};
  const span = {
    setAttribute: (key: string, value: unknown) => {
      attributes[key] = value;
    },
    setStatus: () => span,
    recordException: () => {},
    end: () => {},
    spanContext: () => ({
      traceId: "4bf92f3577b34da6a3ce929d0e0e4736",
      spanId: "00f067aa0ba902b7",
      traceFlags: 1,
    }),
  } as unknown as Span;
  const tracer = {
    startActiveSpan: (
      _name: string,
      options: { attributes?: Record<string, unknown> },
      fn: (s: Span) => unknown,
    ) => {
      Object.assign(attributes, options.attributes);
      return fn(span);
    },
  } as unknown as Tracer;
  return { tracer, attributes };
}

function fakeReq(header: Headers): UnaryRequest {
  return {
    stream: false,
    service: { typeName: "test.EchoService" },
    method: { name: "Say" },
    header,
    message: {},
  } as unknown as UnaryRequest;
}

function fakeRes(header: Headers): UnaryResponse {
  return {
    stream: false,
    service: { typeName: "test.EchoService" },
    method: { name: "Say" },
    header,
    message: {},
  } as unknown as UnaryResponse;
}

describe("withTracing", () => {
  it("records the request x-request-id as the request_id span attribute", async () => {
    const { tracer, attributes } = fakeTracer();
    const interceptor = withTracing({ tracer });
    const req = fakeReq(new Headers({ "x-request-id": "req-123" }));
    await interceptor((r) => Promise.resolve(fakeRes(new Headers())))(req);
    expect(attributes["request_id"]).toBe("req-123");
  });

  it("records the server-echoed x-request-id when the request had none", async () => {
    const { tracer, attributes } = fakeTracer();
    const interceptor = withTracing({ tracer });
    const req = fakeReq(new Headers());
    await interceptor((r) =>
      Promise.resolve(fakeRes(new Headers({ "x-request-id": "srv-456" })))
    )(req);
    expect(attributes["request_id"]).toBe("srv-456");
  });

  it("keeps the request x-request-id when the server echoes a different one", async () => {
    const { tracer, attributes } = fakeTracer();
    const interceptor = withTracing({ tracer });
    const req = fakeReq(new Headers({ "x-request-id": "req-123" }));
    await interceptor((r) =>
      Promise.resolve(fakeRes(new Headers({ "x-request-id": "srv-456" })))
    )(req);
    expect(attributes["request_id"]).toBe("req-123");
  });

  it("injects trace context headers into the request", async () => {
    const testPropagator: TextMapPropagator = {
      inject: (_ctx, carrier, setter) => {
        setter.set(
          carrier,
          "traceparent",
          "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        );
      },
      extract: (ctx) => ctx,
      fields: () => ["traceparent"],
    };
    propagation.setGlobalPropagator(testPropagator);
    const { tracer } = fakeTracer();
    const interceptor = withTracing({ tracer });
    const req = fakeReq(new Headers());
    await interceptor((r) => Promise.resolve(fakeRes(new Headers())))(req);
    expect(req.header.get("traceparent")).toBe(
      "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
    );
  });
});
