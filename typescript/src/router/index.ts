/**
 * TanStack Router integration for the Sunbeam G2V framework.
 *
 * Provides route guards, auth-gated routes, RPC route loaders, and prefetch
 * utilities that wire the framework's transport, auth stores, and query client
 * into TanStack Router v1.
 *
 * @example
 * ```ts
 * import { withAuthGuard, createRpcLoaderWithTransport } from "@sunbeam/g2v/router";
 *
 * const dashboardRoute = createRoute({
 *   getParentRoute: () => rootRoute,
 *   path: "/dashboard",
 *   beforeLoad: withAuthGuard({ redirectTo: "/login" }),
 *   loader: ({ context }) =>
 *     createRpcLoaderWithTransport(context.transport, StatsService, "getStats", {})({
 *       context,
 *     }),
 *   component: DashboardPage,
 * });
 * ```
 *
 * @module
 */

export * from "./guards.ts";
export * from "./RouteAuthGuard.tsx";
export * from "./loaders.ts";
export * from "./prefetch.ts";
export * from "./ServiceErrorBoundary.tsx";
