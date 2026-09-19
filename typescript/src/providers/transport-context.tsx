import { type Transport } from "@connectrpc/connect";
import { createContext, type Context, useContext } from "react";

const TransportContext: Context<Transport | null> = createContext<Transport | null>(null);

/**
 * Provider component for injecting the Connect RPC transport into the React context tree.
 * Use as a component: `<TransportProvider value={transport}>...</TransportProvider>`
 */
export const TransportProvider = TransportContext.Provider;

/**
 * Hook to retrieve the Connect RPC transport from context.
 * Must be called within a <FrameworkProvider> or <TransportProvider> subtree.
 *
 * @returns The Connect RPC Transport instance.
 * @throws Error if called outside of a TransportProvider.
 *
 * @example
 * ```ts
 * const transport = useTransport();
 * const client = createClient(MyService, transport);
 * ```
 */
export function useTransport(): Transport {
  const t = useContext(TransportContext);
  if (!t) {
    throw new Error(
      "sunbeam-g2v: useTransport must be used inside <FrameworkProvider>",
    );
  }
  return t;
}
