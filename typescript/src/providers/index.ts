/**
 * React context providers for transport, data fetching, observability, and auth.
 *
 * Exports composable providers for Connect RPC transport, TanStack Query,
 * OpenTelemetry, and authentication. Wrap your app tree with FrameworkProvider
 * (or compose individual providers) to enable full g2v integration.
 *
 * @example
 * ```tsx
 * import { FrameworkProvider, createTransport } from "@sunbeam/g2v/providers";
 *
 * const root = createRoot(document.getElementById("root")!);
 * root.render(
 *   <FrameworkProvider transport={createTransport({ baseUrl: "..." })}>
 *     <App />
 *   </FrameworkProvider>
 * );
 * ```
 *
 * @module
 */

export * from "./FrameworkProvider.tsx";
export * from "./QueryProvider.tsx";
export * from "./OtelProvider.tsx";
export * from "./AuthProvider.tsx";
export * from "./NotificationProvider.tsx";
export * from "./transport-context.tsx";
