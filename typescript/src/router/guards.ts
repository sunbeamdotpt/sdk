import { redirect } from "@tanstack/react-router";
import { authSelectors, authStore } from "../state/auth.ts";

/**
 * Options for auth guards.
 */
export interface AuthGuardOptions {
  /** Route to redirect unauthenticated users to. Defaults to "/login". */
  redirectTo?: string;
}

/**
 * Creates a TanStack Router `beforeLoad` guard that requires authentication.
 * Redirects anonymous or expired sessions to the login page.
 *
 * @example
 * ```ts
 * const protectedRoute = createRoute({
 *   getParentRoute: () => rootRoute,
 *   path: "/dashboard",
 *   beforeLoad: withAuthGuard({ redirectTo: "/login" }),
 *   component: DashboardPage,
 * });
 * ```
 */
export function withAuthGuard(options: AuthGuardOptions = {}): () => void {
  const { redirectTo = "/login" } = options;
  return () => {
    if (!authSelectors.isAuthenticated()) {
      throw redirect({ to: redirectTo });
    }
  };
}

/**
 * Creates a TanStack Router `beforeLoad` guard that requires specific roles.
 * Redirects unauthenticated users to login, and users without the required
 * roles to the fallback route.
 *
 * @example
 * ```ts
 * const adminRoute = createRoute({
 *   getParentRoute: () => rootRoute,
 *   path: "/admin",
 *   beforeLoad: withRoleGuard(["admin"], { redirectTo: "/" }),
 *   component: AdminPage,
 * });
 * ```
 */
export function withRoleGuard(
  roles: string[],
  options: AuthGuardOptions = {},
): () => void {
  const { redirectTo = "/" } = options;
  return () => {
    if (!authSelectors.isAuthenticated()) {
      throw redirect({ to: "/login" });
    }
    const userRoles = authStore.session.get()?.claims?.roles ?? [];
    const hasRole = roles.some((r) => userRoles.includes(r));
    if (!hasRole) {
      throw redirect({ to: redirectTo });
    }
  };
}
