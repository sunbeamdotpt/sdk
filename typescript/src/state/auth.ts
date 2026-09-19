import { observable, type Observable } from "@legendapp/state";

const AUTH_STORAGE_KEY = "sunbeam-g2v:auth";

/**
 * OIDC/JWT claims extracted from an ID token.
 * Contains standard OIDC subject claim plus optional email, name, roles, and additional custom claims.
 */
export interface AuthClaims {
  /** Subject (user ID) from the OIDC provider. */
  sub: string;
  /** User's email address (optional). */
  email?: string;
  /** User's display name (optional). */
  name?: string;
  /** List of role identifiers assigned to the user (optional). */
  roles?: string[];
  /** Additional custom claims. */
  [k: string]: unknown;
}

/**
 * Represents an authenticated session with tokens and optional claims.
 * Persists authentication state including access/refresh tokens and expiry.
 */
export interface AuthSession {
  /** JWT access token for API calls. */
  accessToken: string;
  /** Refresh token for obtaining new access tokens (optional). */
  refreshToken?: string;
  /** Unix timestamp when the access token expires (optional). */
  expiresAt?: number;
  /** Decoded claims from the ID token (optional). */
  claims?: AuthClaims;
}

/**
 * Root state object tracking authentication status, current session, and errors.
 * Represents the full authentication lifecycle: anonymous → authenticating → authenticated/expired.
 */
export interface AuthState {
  /** Current authentication status. */
  status: "anonymous" | "authenticating" | "authenticated" | "expired";
  /** Active session (present when status is "authenticated"). */
  session?: AuthSession;
  /** Error message from last failed login attempt (present when status is "anonymous" after failure). */
  error?: string;
}

/**
 * Observable store tracking the user's authentication state (session, status, errors).
 * Subscribe to track login/logout events and session expiry. Consumers typically
 * check `authStore.status` to gate protected routes and read `authStore.session`
 * to access tokens or claims.
 *
 * @example
 * ```tsx
 * const status = authStore.status.get(); // "authenticated" | "anonymous" | etc.
 * const token = authStore.session.get()?.accessToken;
 * authActions.loginSuccess({ accessToken: "..." });
 * ```
 */
const restoreAuth = (): AuthState => {
  try {
    const raw = globalThis.localStorage?.getItem(AUTH_STORAGE_KEY);
    if (!raw) return { status: "anonymous" };
    const parsed = JSON.parse(raw) as AuthState;
    if (parsed.session?.expiresAt && parsed.session.expiresAt * 1000 < Date.now()) {
      return { status: "expired", session: parsed.session };
    }
    return parsed;
  } catch {
    return { status: "anonymous" };
  }
};

export const authStore: Observable<AuthState> = observable<AuthState>(restoreAuth());

authStore.onChange(({ value }) => {
  try {
    if (value.status === "anonymous") {
      globalThis.localStorage?.removeItem(AUTH_STORAGE_KEY);
    } else {
      globalThis.localStorage?.setItem(AUTH_STORAGE_KEY, JSON.stringify(value));
    }
  } catch {
    /* storage unavailable */
  }
});

/**
 * Shape of the {@link authActions} object.
 */
export interface AuthActions {
  /** Mark the start of a login attempt (status → "authenticating"). */
  beginLogin(): void;
  /** Record a successful login and store the active session. */
  loginSuccess(session: AuthSession): void;
  /** Record a failed login attempt with an error message. */
  loginFailure(error: string): void;
  /** Clear authentication state on user-initiated logout. */
  logout(): void;
  /** Mark the active session as expired. */
  expire(): void;
}

/**
 * Actions to mutate authentication state.
 * Use these to update auth status during login/logout flows or token refresh.
 */
export const authActions: AuthActions = {
  /**
   * Mark the start of a login attempt.
   * Transitions status to "authenticating" to show loading UI.
   */
  beginLogin(): void {
    authStore.set({ status: "authenticating" });
  },

  /**
   * Record a successful login with the returned session.
   * Sets status to "authenticated" and stores the session (tokens + claims).
   *
   * @param session - The authenticated session with tokens.
   */
  loginSuccess(session: AuthSession): void {
    authStore.set({ status: "authenticated", session });
  },

  /**
   * Record a failed login attempt.
   * Reverts status to "anonymous" and stores the error message.
   *
   * @param error - Human-readable error message from the auth provider.
   */
  loginFailure(error: string): void {
    authStore.set({ status: "anonymous", error });
  },

  /**
   * Clear authentication state (e.g., on user-initiated logout).
   * Resets status to "anonymous" and clears session/error.
   */
  logout(): void {
    authStore.set({ status: "anonymous" });
  },

  /**
   * Mark the session as expired.
   * Transitions status to "expired" (typically triggering a re-auth flow).
   */
  expire(): void {
    authStore.status.set("expired");
  },
};

/**
 * Shape of the {@link authSelectors} object.
 */
export interface AuthSelectors {
  /** True if status is "authenticated". */
  isAuthenticated(): boolean;
  /** Current access token, or undefined if not authenticated. */
  token(): string | undefined;
  /** Current user's OIDC claims, or undefined if not authenticated. */
  claims(): AuthClaims | undefined;
  /** True if the user's roles array includes the given role. */
  hasRole(role: string): boolean;
}

/**
 * Selectors to derive computed auth values.
 * Use these to check login status, extract tokens, or query role membership
 * without directly accessing store state. Selectors are ideal for UI conditions
 * (e.g., `if (authSelectors.isAuthenticated())`) and permission checks.
 */
export const authSelectors: AuthSelectors = {
  /**
   * Check if the user is currently authenticated.
   *
   * @returns True if status is "authenticated".
   */
  isAuthenticated(): boolean {
    return authStore.status.get() === "authenticated";
  },

  /**
   * Get the current access token.
   *
   * @returns The JWT access token, or undefined if not authenticated.
   */
  token(): string | undefined {
    return authStore.session.get()?.accessToken;
  },

  /**
   * Get the current user's claims.
   *
   * @returns The decoded OIDC claims, or undefined if not authenticated.
   */
  claims(): AuthClaims | undefined {
    return authStore.session.get()?.claims;
  },

  /**
   * Check if the user has a specific role.
   *
   * @param role - The role identifier to check.
   * @returns True if the user's roles array includes the role.
   */
  hasRole(role: string): boolean {
    return Boolean(authStore.session.get()?.claims?.roles?.includes(role));
  },
};
