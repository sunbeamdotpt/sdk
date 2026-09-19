import React, { type ReactNode } from "react";
import { ServiceError, isServiceError } from "../core/errors.ts";

/**
 * Maps a ServiceError kind to a fallback React node.
 */
export type ErrorFallbackMap = Partial<
  Record<ServiceError["kind"], ReactNode>
> & {
  /** Fallback when no specific kind matches. */
  default?: ReactNode;
};

/**
 * Props for {@link ServiceErrorBoundary}.
 */
export interface ServiceErrorBoundaryProps {
  /** Content to render when no error has occurred. */
  children: ReactNode;
  /** Fallback UI keyed by error kind. */
  fallback: ErrorFallbackMap;
}

/**
 * Simple error boundary that catches errors in its subtree and renders
 * fallback UI based on the {@link ServiceError} kind.
 *
 * This is a lightweight component-level boundary. For route-level error
 * handling, use TanStack Router's `errorComponent` with {@link isServiceError}.
 *
 * @example
 * ```tsx
 * <ServiceErrorBoundary
 *   fallback={{
 *     Unauthenticated: <RedirectToLogin />,
 *     PermissionDenied: <ForbiddenPage />,
 *     default: <GenericErrorPage />,
 *   }}
 * >
 *   <DataHeavyComponent />
 * </ServiceErrorBoundary>
 * ```
 */
export class ServiceErrorBoundary extends React.Component<
  ServiceErrorBoundaryProps,
  { hasError: boolean; error: unknown }
> {
  constructor(props: ServiceErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: unknown): { hasError: boolean; error: unknown } {
    return { hasError: true, error };
  }

  override render(): ReactNode {
    if (!this.state.hasError) {
      return this.props.children;
    }

    const err = this.state.error;
    if (isServiceError(err)) {
      return this.props.fallback[err.kind] ?? this.props.fallback.default ?? null;
    }

    return this.props.fallback.default ?? null;
  }
}
