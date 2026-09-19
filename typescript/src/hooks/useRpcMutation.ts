import { type DescService } from "@bufbuild/protobuf";
import {
  useMutation,
  type UseMutationOptions,
  type UseMutationResult,
} from "@tanstack/react-query";
import { ServiceError } from "../core/errors.ts";
import {
  type RpcInput,
  type RpcOutput,
  type UnaryMethodNames,
  useRpcClient,
} from "./useRpcQuery.ts";

/**
 * Configuration options for useRpcMutation hook.
 * Extends TanStack Query mutation options, omitting mutationFn which is managed automatically.
 */
export type RpcMutationOptions<TData, TVariables> = Omit<
  UseMutationOptions<TData, ServiceError, TVariables>,
  "mutationFn"
>;

/**
 * Hook to call a unary RPC method for mutations using TanStack Query.
 * Automatically creates a client for the service, handles errors, and manages mutation state.
 * Mutations do not retry by default (as configured in the QueryProvider).
 *
 * @param service - The protobuf service descriptor.
 * @param method - The name of the unary RPC method to call.
 * @param options - Optional mutation options (onSuccess, onError, etc.).
 * @returns A TanStack Query mutation result object with mutate function and status properties.
 *
 * @example
 * ```tsx
 * const { mutate, isPending } = useRpcMutation(UsersService, "createUser");
 *
 * return (
 *   <button onClick={() => mutate({ name: "John" })} disabled={isPending}>
 *     Create User
 *   </button>
 * );
 * ```
 */
export function useRpcMutation<
  S extends DescService,
  K extends UnaryMethodNames<S>,
>(
  service: S,
  method: K,
  options: RpcMutationOptions<RpcOutput<S, K>, RpcInput<S, K>> = {},
): UseMutationResult<RpcOutput<S, K>, ServiceError, RpcInput<S, K>> {
  const client = useRpcClient(service);
  return useMutation<RpcOutput<S, K>, ServiceError, RpcInput<S, K>>({
    ...options,
    mutationFn: async (request) => {
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
