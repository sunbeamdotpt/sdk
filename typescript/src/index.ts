/**
 * Sunbeam g2v — main entry point for browser clients.
 *
 * Named for the Sun's spectral classification (G2V, a main-sequence
 * yellow dwarf): the framework that powers everything else.
 *
 * Re-exports transport, interceptors, state stores, OpenTelemetry setup,
 * React providers, and data-fetching hooks. Provides a complete integration
 * layer for Connect RPC services with legend-state, TanStack Query, and
 * browser observability.
 *
 * @example
 * ```tsx
 * import {
 *   FrameworkProvider,
 *   createTransport,
 *   useRpcQuery,
 * } from "@sunbeam/g2v";
 *
 * function App() {
 *   const transport = createTransport({ baseUrl: "https://api.example.com" });
 *   return (
 *     <FrameworkProvider transport={transport}>
 *       <YourApp />
 *     </FrameworkProvider>
 *   );
 * }
 * ```
 *
 * @module
 */

export * from "./core/index.ts";
export * from "./interceptors/index.ts";
export * from "./state/index.ts";
export * from "./otel/index.ts";
export * from "./providers/index.ts";
export * from "./hooks/index.ts";
export * from "./router/index.ts";
export * from "./rest/index.ts";
