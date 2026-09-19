import {
  useQuery,
  useMutation,
  type UseQueryOptions,
  type UseQueryResult,
  type UseMutationOptions,
  type UseMutationResult,
} from "@tanstack/react-query";
import { useMemo } from "react";
import { ServiceError } from "../core/errors.ts";
import { createRestClient, type RestClient, type RestRequestOptions } from "./client.ts";

/**
 * Options for {@link useRestQuery}.
 */
export interface RestQueryOptions<TData>
  extends Omit<
    UseQueryOptions<TData, ServiceError, TData, readonly unknown[]>,
    "queryKey" | "queryFn"
  > {
  /** Override the default `["rest", method, path]` cache key. */
  queryKey?: readonly unknown[];
}

/**
 * Hook to fetch data from a REST endpoint using TanStack Query.
 * Automatically injects auth headers, normalizes errors to {@link ServiceError},
 * and manages cache state.
 *
 * @param client - REST client (from {@link createRestClient}) or base URL string.
 * @param path - API path (e.g. "/users/123").
 * @param options - Optional query options (staleTime, enabled, etc.).
 * @returns TanStack Query result with data, status, error, and refetch.
 *
 * @example
 * ```tsx
 * const api = createRestClient({ baseUrl: "https://api.example.com" });
 * const { data, isLoading } = useRestQuery(api, "/users/123");
 * ```
 */
export function useRestQuery<TData>(
  client: RestClient | string,
  path: string,
  options: RestQueryOptions<TData> = {},
): UseQueryResult<TData, ServiceError> {
  const restClient = useMemo(
    () => (typeof client === "string" ? createRestClient({ baseUrl: client }) : client),
    [client],
  );

  const queryKey = options.queryKey ?? ["rest", "GET", path];

  return useQuery<TData, ServiceError, TData, readonly unknown[]>({
    ...options,
    queryKey,
    queryFn: async () => {
      try {
        return await restClient.get<TData>(path);
      } catch (err) {
        throw ServiceError.from(err);
      }
    },
  });
}

/**
 * Options for {@link useRestMutation}.
 */
export interface RestMutationOptions<TData, TVariables = RestRequestOptions>
  extends Omit<UseMutationOptions<TData, ServiceError, TVariables>, "mutationFn"> {}

/**
 * Hook to perform a REST mutation (POST, PUT, PATCH, DELETE) using TanStack Query.
 * Automatically injects auth headers and normalizes errors to {@link ServiceError}.
 *
 * @param client - REST client (from {@link createRestClient}) or base URL string.
 * @param method - HTTP method for the mutation.
 * @param path - API path (e.g. "/users").
 * @param options - Optional mutation options.
 * @returns TanStack Mutation result with mutate, status, error, etc.
 *
 * @example
 * ```tsx
 * const api = createRestClient({ baseUrl: "https://api.example.com" });
 * const createUser = useRestMutation(api, "POST", "/users");
 * createUser.mutate({ body: { name: "Alice" } });
 * ```
 */
export function useRestMutation<TData, TVariables = RestRequestOptions>(
  client: RestClient | string,
  method: "POST" | "PUT" | "PATCH" | "DELETE",
  path: string,
  options: RestMutationOptions<TData, TVariables> = {},
): UseMutationResult<TData, ServiceError, TVariables> {
  const restClient = useMemo(
    () => (typeof client === "string" ? createRestClient({ baseUrl: client }) : client),
    [client],
  );

  return useMutation<TData, ServiceError, TVariables>({
    ...options,
    mutationFn: async (variables) => {
      const opts = (variables as RestRequestOptions) ?? {};
      try {
        switch (method) {
          case "POST":
            return await restClient.post<TData>(path, opts);
          case "PUT":
            return await restClient.put<TData>(path, opts);
          case "PATCH":
            return await restClient.patch<TData>(path, opts);
          case "DELETE":
            return await restClient.del<TData>(path, opts);
        }
      } catch (err) {
        throw ServiceError.from(err);
      }
    },
  });
}
