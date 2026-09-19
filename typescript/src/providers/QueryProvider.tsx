import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useMemo, type ReactNode } from "react";
import { ServiceError } from "../core/errors.ts";

/**
 * Props for the QueryProvider component.
 * @property children - React nodes to render within the provider.
 * @property client - Optional custom TanStack Query client. If omitted, a default client is created.
 */
export interface QueryProviderProps {
  /** React subtree that should access the QueryClient. */
  children: ReactNode;
  /** Caller-supplied QueryClient (defaults to defaultQueryClient). */
  client?: QueryClient;
}

/**
 * Create a default TanStack Query client with sensible defaults for RPC queries and mutations.
 * Configures 30 second stale time, automatic retry on retryable errors for queries,
 * and no retry for mutations.
 *
 * @returns A new QueryClient instance with default options.
 *
 * @example
 * ```ts
 * const client = defaultQueryClient();
 * ```
 */
export function defaultQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        retry: (failureCount, error) =>
          failureCount < 2 && ServiceError.from(error).retryable,
      },
      mutations: {
        retry: false,
      },
    },
  });
}

/**
 * Provider component that configures TanStack Query for the application.
 * Uses a custom client if provided, otherwise creates a default client.
 *
 * @param props - Component props containing children and optional query client.
 * @returns JSX element rendering the QueryClientProvider with children.
 */
export function QueryProvider({ children, client }: QueryProviderProps): ReactNode {
  const qc = useMemo(() => client ?? defaultQueryClient(), [client]);
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}
