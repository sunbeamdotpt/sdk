import {
  type DescMessage,
  type DescMethodUnary,
  type DescService,
  type MessageInitShape,
  type MessageShape,
} from "@bufbuild/protobuf";
import { createClient, type Transport } from "@connectrpc/connect";
import type { QueryClient } from "@tanstack/react-query";
import { ServiceError } from "../core/errors.ts";
import type { UnaryMethodNames } from "../hooks/useRpcQuery.ts";

/**
 * Context shape injected into TanStack Router route context by the framework.
 * Add this to your root route's `beforeLoad` or `loader` context type.
 *
 * @example
 * ```ts
 * interface MyRouteContext {
 *   queryClient: QueryClient;
 *   transport: Transport;
 * }
 * ```
 */
export interface RpcRouteContext {
  queryClient: QueryClient;
  transport?: Transport;
}

/**
 * Creates a TanStack Router `loader` that prefetches an RPC query before the
 * route renders. The prefetched data is injected into the route's query client
 * so that the matching `useRpcQuery` call on the page hydrates instantly.
 *
 * @param service - The protobuf service descriptor.
 * @param method - The unary RPC method name.
 * @param input - The request input.
 * @returns A route loader function.
 *
 * @example
 * ```ts
 * const userRoute = createRoute({
 *   getParentRoute: () => rootRoute,
 *   path: "/users/$userId",
 *   loader: ({ params, context }) =>
 *     createRpcLoader(UsersService, "getUser", { id: params.userId })({ context }),
 *   component: UserPage,
 * });
 * ```
 */
export function createRpcLoader<
  S extends DescService,
  K extends UnaryMethodNames<S>,
>(
  service: S,
  method: K,
  input: S["method"][K] extends DescMethodUnary<infer I, DescMessage>
    ? MessageInitShape<I>
    : never,
): (args: { context: RpcRouteContext }) => Promise<
  S["method"][K] extends DescMethodUnary<DescMessage, infer O>
    ? MessageShape<O>
    : never
> {
  return async ({
    context,
  }: {
    context: RpcRouteContext;
  }): Promise<
    S["method"][K] extends DescMethodUnary<DescMessage, infer O>
      ? MessageShape<O>
      : never
  > => {
    const queryClient = context.queryClient;
    const transport = context.transport;
    if (!transport) {
      throw new ServiceError(
        "Configuration",
        "createRpcLoader requires a transport in route context. " +
          "Set `transport` in your root route's `beforeLoad` and pass it through context.",
      );
    }
    const queryKey = [service.typeName, method, input];

    return queryClient.fetchQuery({
      queryKey,
      queryFn: async () => {
        const client = createClient(service, transport);
        const fn = client[method] as unknown as (
          r: S["method"][K] extends DescMethodUnary<infer I, DescMessage>
            ? MessageInitShape<I>
            : never,
        ) => Promise<
          S["method"][K] extends DescMethodUnary<DescMessage, infer O>
            ? MessageShape<O>
            : never
        >;
        try {
          return await fn(input);
        } catch (err) {
          throw ServiceError.from(err);
        }
      },
    });
  };
}

/**
 * Same as {@link createRpcLoader} but accepts a transport directly.
 * Use this when you have access to a transport outside React context
 * (e.g., from a singleton or from route context).
 *
 * @param transport - The ConnectRPC transport.
 * @param service - The protobuf service descriptor.
 * @param method - The unary RPC method name.
 * @param input - The request input.
 * @returns A route loader function.
 */
export function createRpcLoaderWithTransport<
  S extends DescService,
  K extends UnaryMethodNames<S>,
>(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  transport: any,
  service: S,
  method: K,
  input: S["method"][K] extends DescMethodUnary<infer I, DescMessage>
    ? MessageInitShape<I>
    : never,
): (args: { context: RpcRouteContext }) => Promise<
  S["method"][K] extends DescMethodUnary<DescMessage, infer O>
    ? MessageShape<O>
    : never
> {
  return async ({
    context,
  }: {
    context: RpcRouteContext;
  }): Promise<
    S["method"][K] extends DescMethodUnary<DescMessage, infer O>
      ? MessageShape<O>
      : never
  > => {
    const queryClient = context.queryClient;
    const queryKey = [service.typeName, method, input];

    return queryClient.fetchQuery({
      queryKey,
      queryFn: async () => {
        const client = createClient(service, transport);
        const fn = client[method] as unknown as (
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          r: any,
        ) => Promise<
          S["method"][K] extends DescMethodUnary<DescMessage, infer O>
            ? MessageShape<O>
            : never
        >;
        try {
          return await fn(input);
        } catch (err) {
          throw ServiceError.from(err);
        }
      },
    });
  };
}
