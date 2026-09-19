import {
  type DescMessage,
  type DescMethodUnary,
  type DescService,
  type MessageInitShape,
  type MessageShape,
} from "@bufbuild/protobuf";
import { createClient, type Client, type Transport } from "@connectrpc/connect";
import {
  useQuery,
  type UseQueryOptions,
  type UseQueryResult,
} from "@tanstack/react-query";
import { useMemo } from "react";
import { ServiceError } from "../core/errors.ts";
import { useTransport } from "../providers/transport-context.tsx";

/**
 * Extracts the names of unary RPC methods from a service descriptor.
 * Filters out streaming and server-streaming methods.
 */
export type UnaryMethodNames<S extends DescService> = {
  [K in keyof S["method"]]: S["method"][K] extends DescMethodUnary<DescMessage, DescMessage>
    ? K
    : never;
}[keyof S["method"]] &
  string;

/**
 * Extracts the input message type for a unary RPC method.
 * Returns the MessageInitShape (constructor form) for the input message.
 */
export type RpcInput<
  S extends DescService,
  K extends UnaryMethodNames<S>,
> = S["method"][K] extends DescMethodUnary<infer I, DescMessage>
  ? MessageInitShape<I>
  : never;

/**
 * Extracts the output message type for a unary RPC method.
 * Returns the MessageShape (runtime form) for the output message.
 */
export type RpcOutput<
  S extends DescService,
  K extends UnaryMethodNames<S>,
> = S["method"][K] extends DescMethodUnary<DescMessage, infer O>
  ? MessageShape<O>
  : never;

/**
 * Create or retrieve a Connect RPC client for a service descriptor.
 * Memoizes the client and re-creates only when the service or transport changes.
 *
 * @param service - The protobuf service descriptor.
 * @param transport - Optional override transport. If omitted, uses the transport from context.
 * @returns A Connect RPC Client instance for the service.
 */
export function useRpcClient<S extends DescService>(
  service: S,
  transport?: Transport,
): Client<S> {
  const ctxTransport = useTransport();
  const t = transport ?? ctxTransport;
  return useMemo(() => createClient(service, t), [service, t]);
}

/**
 * Configuration options for useRpcQuery hook.
 * Extends TanStack Query options, omitting queryKey and queryFn which are managed automatically.
 * @property queryKey - Optional custom query key. If omitted, defaults to [service.typeName, method, request].
 */
export interface RpcQueryOptions<TData, TError = ServiceError>
  extends Omit<
    UseQueryOptions<TData, TError, TData, readonly unknown[]>,
    "queryKey" | "queryFn"
  > {
  /** Override the default `[service, method, input]` cache key. */
  queryKey?: readonly unknown[];
}

/**
 * Hook to fetch data from a unary RPC method using TanStack Query.
 * Automatically creates a client for the service, configures retry logic for retryable errors,
 * and manages query state.
 *
 * @param service - The protobuf service descriptor.
 * @param method - The name of the unary RPC method to call.
 * @param request - The request message (input).
 * @param options - Optional query options (staleTime, refetchInterval, enabled, etc.).
 * @returns A TanStack Query result object with data, status, error, and refetch methods.
 *
 * @example
 * ```tsx
 * const { data, isLoading, error } = useRpcQuery(
 *   UsersService,
 *   "getUser",
 *   { id: "123" },
 *   { staleTime: 60_000 }
 * );
 * ```
 */
export function useRpcQuery<S extends DescService, K extends UnaryMethodNames<S>>(
  service: S,
  method: K,
  request: RpcInput<S, K>,
  options: RpcQueryOptions<RpcOutput<S, K>> = {},
): UseQueryResult<RpcOutput<S, K>, ServiceError> {
  const client = useRpcClient(service);
  const queryKey = options.queryKey ?? [service.typeName, method, request];
  return useQuery<RpcOutput<S, K>, ServiceError, RpcOutput<S, K>, readonly unknown[]>({
    ...options,
    queryKey,
    queryFn: async () => {
      try {
        const fn = client[method] as unknown as (
          r: RpcInput<S, K>,
        ) => Promise<RpcOutput<S, K>>;
        return await fn(request);
      } catch (err) {
        throw ServiceError.from(err);
      }
    },
  });
}
