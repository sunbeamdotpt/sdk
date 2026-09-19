/**
 * Legend-state reactive stores for authentication, notifications, and UI state.
 *
 * Exports immutable, observable stores with actions and selectors for managing
 * auth sessions/claims, toast notifications, and UI preferences (theme, density).
 * Stores integrate with React components via hooks and provide fine-grained
 * reactivity for minimal re-renders.
 *
 * @example
 * ```tsx
 * import { authStore, authActions, notificationsStore } from "@sunbeam/g2v/state";
 *
 * authActions.login({ token: "..." });
 * const user = authStore.user.peek();
 * notificationsStore.add({ message: "Saved", level: "success" });
 * ```
 *
 * @module
 */

export * from "./auth.ts";
export * from "./notifications.ts";
export * from "./ui.ts";
