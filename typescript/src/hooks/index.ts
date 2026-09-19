/**
 * React hooks for data fetching, mutations, authentication, and theme management.
 *
 * Exports hooks for querying and mutating RPC methods (TanStack Query integration),
 * accessing auth state, reading UI theme preference, and checking service health.
 * Hooks provide reactive updates and automatic cache management.
 *
 * @example
 * ```tsx
 * import { useRpcQuery, useRpcMutation, useAuth } from "@sunbeam/g2v/hooks";
 *
 * function UserCard() {
 *   const { user } = useAuth();
 *   const { data } = useRpcQuery({ method: GetUser, input: { id: user?.id } });
 *   const updateMutation = useRpcMutation({ method: UpdateUser });
 *
 *   return <div>{data?.name}</div>;
 * }
 * ```
 *
 * @module
 */

export * from "./useRpcQuery.ts";
export * from "./useRpcMutation.ts";
export * from "./useRpcStream.ts";
export * from "./useAuth.ts";
export * from "./useTheme.ts";
export * from "./useHealth.ts";
export * from "./useTokenRefresh.ts";
export * from "./useRpcForm.ts";
