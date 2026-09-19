import { type ReactNode } from "react";
import { authSelectors } from "../state/auth.ts";

/**
 * Props for the {@link RouteAuthGuard} component.
 */
export interface RouteAuthGuardProps {
  /** Content to render when the user is authenticated. */
  children: ReactNode;
  /** Content to render when the user is not authenticated. */
  fallback: ReactNode;
}

/**
 * Component-level auth gate. Renders `children` when the user is authenticated,
 * otherwise renders `fallback`.
 *
 * Prefer {@link withAuthGuard} on route `beforeLoad` for route-level protection.
 * This component is useful for gating sections inside a page (e.g., a sidebar
 * widget that should only appear for logged-in users).
 *
 * @example
 * ```tsx
 * <RouteAuthGuard fallback={<LoginPrompt />}>
 *   <UserProfileCard />
 * </RouteAuthGuard>
 * ```
 */
export function RouteAuthGuard({ children, fallback }: RouteAuthGuardProps): ReactNode {
  return authSelectors.isAuthenticated() ? children : fallback;
}
