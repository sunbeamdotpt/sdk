import { describe, it } from "@std/testing/bdd";
import { expect } from "@std/expect";
import { loadConfig } from "./config.ts";

describe("loadConfig", () => {
  it("falls back to defaults when env is empty", () => {
    const cfg = loadConfig({ env: {} });
    expect(cfg.serviceName).toBe("sunbeam-app");
    expect(cfg.transport.baseUrl).toBe("/api");
    expect(cfg.environment).toBe("development");
    expect(cfg.limits?.maxRetries).toBe(3);
  });

  it("reads SUNBEAM_* env vars", () => {
    const cfg = loadConfig({
      env: {
        SUNBEAM_SERVICE_NAME: "kanban",
        SUNBEAM_API_BASE_URL: "https://api.example.com",
        SUNBEAM_OTLP_URL: "https://otel.example.com/v1/traces",
        SUNBEAM_MAX_RETRIES: "5",
        SUNBEAM_USE_BINARY: "true",
      },
    });
    expect(cfg.serviceName).toBe("kanban");
    expect(cfg.transport.baseUrl).toBe("https://api.example.com");
    expect(cfg.observability?.otlpUrl).toBe("https://otel.example.com/v1/traces");
    expect(cfg.limits?.maxRetries).toBe(5);
    expect(cfg.transport.useBinaryFormat).toBe(true);
  });

  it("VITE_-prefixed env vars take precedence", () => {
    const cfg = loadConfig({
      env: {
        VITE_SUNBEAM_SERVICE_NAME: "vite-app",
        SUNBEAM_SERVICE_NAME: "fallback",
      },
    });
    expect(cfg.serviceName).toBe("vite-app");
  });

  it("merges defaults override of unset env values", () => {
    const cfg = loadConfig({
      env: {},
      defaults: { serviceName: "custom", transport: { baseUrl: "/v2" } },
    });
    expect(cfg.serviceName).toBe("custom");
    expect(cfg.transport.baseUrl).toBe("/v2");
  });
});
