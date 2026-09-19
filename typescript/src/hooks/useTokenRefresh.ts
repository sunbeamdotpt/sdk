import { useEffect, useRef } from "react";
import { authActions, authStore, type AuthSession } from "../state/auth.ts";

/**
 * Options for the useTokenRefresh hook.
 */
export interface TokenRefreshOptions {
  /** Function that receives the current session and returns a new session with fresh tokens. */
  refresh: (session: AuthSession) => Promise<AuthSession>;
  /** Seconds before expiry to trigger refresh (default: 60). */
  bufferSeconds?: number;
  /** Callback invoked when refresh fails. */
  onError?: (error: unknown) => void;
}

/**
 * Automatically refreshes the access token before it expires.
 *
 * Watches the auth store session and sets a timer to refresh the token
 * `bufferSeconds` before `expiresAt`. If no `expiresAt` is present,
 * no timer is set.
 *
 * @example
 * ```tsx
 * function App() {
 *   useTokenRefresh({
 *     refresh: async (session) => {
 *       const res = await fetch("/api/refresh", {
 *         headers: { Authorization: `Bearer ${session.refreshToken}` },
 *       });
 *       return res.json();
 *     },
 *     onError: () => authActions.expire(),
 *   });
 *   return <YourApp />;
 * }
 * ```
 */
export function useTokenRefresh(options: TokenRefreshOptions): void {
  const { refresh, bufferSeconds = 60, onError } = options;
  const refreshRef = useRef(refresh);
  const onErrorRef = useRef(onError);
  refreshRef.current = refresh;
  onErrorRef.current = onError;

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;

    const schedule = () => {
      if (timer) clearTimeout(timer);
      const session = authStore.session.get();
      if (!session?.expiresAt) return;

      const expiresAtMs = session.expiresAt * 1000;
      const refreshAt = expiresAtMs - bufferSeconds * 1000;
      const delay = refreshAt - Date.now();

      if (delay <= 0) {
        // Already expired or within buffer — refresh immediately
        doRefresh(session);
        return;
      }

      timer = setTimeout(() => {
        const current = authStore.session.get();
        if (current) doRefresh(current);
      }, delay);
    };

    const doRefresh = async (session: AuthSession) => {
      try {
        const next = await refreshRef.current(session);
        authActions.loginSuccess(next);
      } catch (err) {
        onErrorRef.current?.(err);
      }
    };

    // Schedule on mount and re-schedule whenever session changes
    schedule();
    const dispose = authStore.session.onChange(schedule);

    return () => {
      if (timer) clearTimeout(timer);
      dispose();
    };
  }, [bufferSeconds]);
}
