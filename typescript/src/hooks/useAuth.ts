import { useSelector } from "@legendapp/state/react";
import { authActions, authStore, type AuthSession } from "../state/auth.ts";

/**
 * Hook to access the current authentication state and actions.
 * Provides the authentication status, session, access token, claims, and login/logout functions.
 *
 * @returns Object containing:
 *   - status: The authentication status ("authenticated", "unauthenticated", "expired", "initializing").
 *   - session: The current AuthSession if authenticated, undefined otherwise.
 *   - isAuthenticated: Boolean convenience flag, true if status === "authenticated".
 *   - token: The access token string if authenticated, undefined otherwise.
 *   - claims: The JWT claims object if authenticated, undefined otherwise.
 *   - login: Function to set an authenticated session.
 *   - logout: Function to clear the session and reset authentication state.
 *
 * @example
 * ```tsx
 * export function MyComponent() {
 *   const { isAuthenticated, token, logout } = useAuth();
 *
 *   if (!isAuthenticated) return <LoginForm />;
 *
 *   return <button onClick={logout}>Logout</button>;
 * }
 * ```
 */
export function useAuth(): {
  status: string;
  session: AuthSession | undefined;
  isAuthenticated: boolean;
  token: string | undefined;
  claims: unknown;
  login: (session: AuthSession) => void;
  logout: () => void;
} {
  const status = useSelector(() => authStore.status.get());
  const session = useSelector(() => authStore.session.get());
  return {
    status,
    session,
    isAuthenticated: status === "authenticated",
    token: session?.accessToken,
    claims: session?.claims,
    login: authActions.loginSuccess,
    logout: authActions.logout,
  };
}
