import { type Transport } from "@connectrpc/connect";
import { type QueryClient } from "@tanstack/react-query";
import { type ReactNode } from "react";
import { type OtelSetupOptions } from "../otel/setup.ts";
import { type AuthSession } from "../state/auth.ts";
import { AuthProvider } from "./AuthProvider.tsx";
import { OtelProvider } from "./OtelProvider.tsx";
import { QueryProvider } from "./QueryProvider.tsx";
import { TransportProvider } from "./transport-context.tsx";

/**
 * Props for the FrameworkProvider component.
 * @property children - React nodes to render within the provider tree.
 * @property transport - The Connect RPC transport for making gRPC calls.
 * @property queryClient - Optional TanStack Query client. If omitted, a default client is created.
 * @property otel - Optional OpenTelemetry configuration. If provided, traces are collected and exported.
 * @property initialSession - Optional initial authentication session to set on mount.
 * @property onSessionExpire - Optional callback invoked when the authentication session expires.
 */
export interface FrameworkProviderProps {
  /** React subtree wrapped by the framework providers. */
  children: ReactNode;
  /** Connect RPC transport injected into TransportProvider. */
  transport: Transport;
  /** TanStack QueryClient (defaults to a sensible client when omitted). */
  queryClient?: QueryClient;
  /** When provided, sets up browser OpenTelemetry on mount. */
  otel?: OtelSetupOptions;
  /** Pre-populates the auth store on mount (useful for SSR hydration). */
  initialSession?: AuthSession;
  /** Invoked when the auth store transitions to "expired". */
  onSessionExpire?: () => void;
}

/**
 * Root provider component that wires together all framework dependencies.
 * Configures transport, query client, authentication, and OpenTelemetry in a single component.
 *
 * @param props - Component props containing transport, optional query client, auth session, and otel config.
 * @returns JSX element rendering the provider tree and children.
 *
 * @example
 * ```tsx
 * <FrameworkProvider
 *   transport={createConnectTransport()}
 *   otel={{ serviceName: "my-app" }}
 *   onSessionExpire={() => navigate("/login")}
 * >
 *   <App />
 * </FrameworkProvider>
 * ```
 */
export function FrameworkProvider({
  children,
  transport,
  queryClient,
  otel,
  initialSession,
  onSessionExpire,
}: FrameworkProviderProps): ReactNode {
  const tree = (
    <TransportProvider value={transport}>
      <QueryProvider client={queryClient}>
        <AuthProvider
          initialSession={initialSession}
          onExpire={onSessionExpire}
        >
          {children}
        </AuthProvider>
      </QueryProvider>
    </TransportProvider>
  );
  return otel ? <OtelProvider {...otel}>{tree}</OtelProvider> : tree;
}
