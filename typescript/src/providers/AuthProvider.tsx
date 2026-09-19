import { type ReactNode, useEffect } from "react";
import {
  authActions,
  authStore,
  type AuthSession,
} from "../state/auth.ts";

/**
 * Props for the AuthProvider component.
 * @property children - React nodes to render within the provider.
 * @property initialSession - Optional initial authentication session to hydrate on mount.
 * @property onExpire - Optional callback to invoke when the authentication session expires.
 */
export interface AuthProviderProps {
  /** React subtree that should observe the auth store. */
  children: ReactNode;
  /** Pre-populates the auth store on mount (e.g., for SSR hydration). */
  initialSession?: AuthSession;
  /** Fired once when the auth store transitions to "expired". */
  onExpire?: () => void;
}

/**
 * Provider component that manages authentication state and session lifecycle.
 * Handles session initialization and expiration callbacks.
 *
 * @param props - Component props containing children, initial session, and expiration callback.
 * @returns JSX element rendering children without additional wrapper elements.
 *
 * @example
 * ```tsx
 * <AuthProvider
 *   initialSession={session}
 *   onExpire={() => navigate("/login")}
 * >
 *   <App />
 * </AuthProvider>
 * ```
 */
export function AuthProvider({
  children,
  initialSession,
  onExpire,
}: AuthProviderProps): ReactNode {
  useEffect(() => {
    if (initialSession) authActions.loginSuccess(initialSession);
  }, [initialSession]);

  useEffect(() => {
    if (!onExpire) return;
    const dispose = authStore.status.onChange(({ value }) => {
      if (value === "expired") onExpire();
    });
    return () => dispose();
  }, [onExpire]);

  return <>{children}</>;
}
