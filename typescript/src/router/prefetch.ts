import { useCallback } from "react";
import {
  type DescMessage,
  type DescService,
  type MessageInitShape,
} from "@bufbuild/protobuf";
import { useQueryClient } from "@tanstack/react-query";
import { useRpcClient } from "../hooks/useRpcQuery.ts";
import type { UnaryMethodNames } from "../hooks/useRpcQuery.ts";

/**
 * Returns a prefetch function for a unary RPC method. Call this on hover or
 * focus to warm the TanStack Query cache before the user navigates.
 *
 * @example
 * ```tsx
 * const prefetchUser = useRoutePrefetch(UsersService, "getUser");
 *
 * <Link
 *   to="/users/$userId"
 *   params={{ userId: user.id }}
 *   onMouseEnter={() => prefetchUser({ id: user.id })}
 * >
 *   {user.name}
 * </Link>
 * ```
 */
export function useRoutePrefetch<S extends DescService, K extends UnaryMethodNames<S>>(
  service: S,
  method: K,
): (request: MessageInitShape<DescMessage>) => void {
  const queryClient = useQueryClient();
  const client = useRpcClient(service);

  return useCallback(
    (request: MessageInitShape<DescMessage>) => {
      const queryKey = [service.typeName, method, request];
      void queryClient.prefetchQuery({
        queryKey,
        queryFn: async () => {
          const fn = client[method] as unknown as (
            r: MessageInitShape<DescMessage>,
          ) => Promise<unknown>;
          return fn(request);
        },
        staleTime: 30_000,
      });
    },
    [queryClient, client, service, method],
  );
}
