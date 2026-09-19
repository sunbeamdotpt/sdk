import { describe, it } from "@std/testing/bdd";
import { expect } from "@std/expect";
import { createClient, type Transport } from "@connectrpc/connect";
import {
  create,
  createFileRegistry,
  type DescService,
} from "@bufbuild/protobuf";
import {
  FieldDescriptorProto_Label,
  FieldDescriptorProto_Type,
  FileDescriptorProtoSchema,
} from "@bufbuild/protobuf/wkt";
import { propagation, type TextMapPropagator } from "@opentelemetry/api";
import { createTransport } from "./transport.ts";

/** Builds a minimal echo service descriptor at runtime (no codegen). */
function echoService(): DescService {
  const stringField = (name: string) => ({
    name,
    number: 1,
    label: FieldDescriptorProto_Label.OPTIONAL,
    type: FieldDescriptorProto_Type.STRING,
    jsonName: name,
  });
  const proto = create(FileDescriptorProtoSchema, {
    name: "echo.proto",
    package: "test",
    syntax: "proto3",
    messageType: [
      { name: "SayRequest", field: [stringField("sentence")] },
      { name: "SayResponse", field: [stringField("sentence")] },
    ],
    service: [{
      name: "EchoService",
      method: [{
        name: "Say",
        inputType: ".test.SayRequest",
        outputType: ".test.SayResponse",
      }],
    }],
  });
  const registry = createFileRegistry(proto, () => undefined);
  const service = registry.getService("test.EchoService");
  if (!service) throw new Error("failed to build test service descriptor");
  return service;
}

/** Strongly-typed view of the runtime-built echo client. */
interface EchoClient {
  say: (req: { sentence: string }) => Promise<{ sentence: string }>;
}

function echoClient(transport: Transport): EchoClient {
  return createClient(echoService(), transport) as unknown as EchoClient;
}

/** Test propagator that always injects a fixed W3C traceparent header. */
const fixedPropagator: TextMapPropagator = {
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

/** Mock fetch that records request headers and replies with a Connect JSON response. */
function recordingFetch(captured: { headers?: Headers }): typeof fetch {
  return (_input, init) => {
    captured.headers = new Headers(init?.headers);
    return Promise.resolve(
      new Response(JSON.stringify({ sentence: "ok" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
  };
}

describe("createTransport", () => {
  it("returns an object with unary/stream methods", () => {
    const t = createTransport({ baseUrl: "/api" });
    expect(typeof t.unary).toBe("function");
    expect(typeof t.stream).toBe("function");
  });

  it("does not throw when interceptors omitted", () => {
    expect(() => createTransport({ baseUrl: "/api" })).not.toThrow();
  });

  it("attaches x-request-id and traceparent by default", async () => {
    propagation.setGlobalPropagator(fixedPropagator);
    const captured: { headers?: Headers } = {};
    const transport = createTransport({
      baseUrl: "http://localhost",
      fetch: recordingFetch(captured),
    });
    const client = echoClient(transport);
    const res = await client.say({ sentence: "hi" });
    expect(res.sentence).toBe("ok");
    expect(captured.headers?.get("x-request-id")).toBeTruthy();
    expect(captured.headers?.get("traceparent")).toBe(
      "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
    );
  });

  it("uses a caller-supplied interceptors array as-is", async () => {
    propagation.setGlobalPropagator(fixedPropagator);
    const captured: { headers?: Headers } = {};
    const transport = createTransport({
      baseUrl: "http://localhost",
      fetch: recordingFetch(captured),
      interceptors: [],
    });
    const client = echoClient(transport);
    await client.say({ sentence: "hi" });
    expect(captured.headers?.get("x-request-id")).toBeNull();
    expect(captured.headers?.get("traceparent")).toBeNull();
  });
});
