import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import {
  authSelectors,
  createTransport,
  FrameworkProvider,
  withAuth,
  withLogging,
  withRequestId,
  withRetryInterceptor,
  withTracing,
} from "sunbeam-g2v";
import { App } from "./App";

const baseUrl =
  (import.meta.env.VITE_CONNECT_BASE_URL as string | undefined) ??
  "http://localhost:8080";

const otlpUrl = import.meta.env.VITE_SUNBEAM_OTLP_URL as string | undefined;

const transport = createTransport({
  baseUrl,
  interceptors: [
    withRequestId(),
    withAuth({ getToken: () => authSelectors.token() }),
    withTracing({ tracerName: "sunbeam-g2v-example" }),
    withLogging(),
    withRetryInterceptor({ onlyIdempotent: true }),
  ],
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <FrameworkProvider
      transport={transport}
      otel={
        otlpUrl
          ? {
              serviceName: "sunbeam-g2v-example",
              otlpUrl,
              propagateTraceHeaderCorsUrls: [/.*/],
            }
          : undefined
      }
    >
      <App />
    </FrameworkProvider>
  </StrictMode>,
);
