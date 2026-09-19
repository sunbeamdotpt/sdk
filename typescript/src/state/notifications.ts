import { observable, type Observable } from "@legendapp/state";

/**
 * Severity level for a notification.
 * Determines styling and icon in the toast UI.
 */
export type NotificationLevel = "info" | "success" | "warning" | "error";

/**
 * A single notification (toast) item.
 * Displayed transiently or until dismissed. Auto-dismisses if ttlMs is set.
 */
export interface Notification {
  /** Unique identifier for this notification (auto-generated). */
  id: string;
  /** Severity level affecting visual presentation. */
  level: NotificationLevel;
  /** Primary message text. */
  title: string;
  /** Optional longer description. */
  body?: string;
  /** Unix timestamp when the notification was created. */
  createdAt: number;
  /** Time in milliseconds before auto-dismissal (optional; if not set, notification persists until manually dismissed). */
  ttlMs?: number;
}

/**
 * State container for all active notifications.
 */
export interface NotificationsState {
  /** Array of currently active notification items. */
  items: Notification[];
}

/**
 * Observable store tracking active toast notifications.
 * Subscribe to react to new notifications. Consumers typically render
 * items as a toast stack and bind dismiss actions to user interactions.
 *
 * @example
 * ```tsx
 * const items = notificationsStore.items.get();
 * notificationActions.push({ level: "success", title: "Saved" });
 * notificationActions.dismiss(id);
 * ```
 */
export const notificationsStore: Observable<NotificationsState> = observable<NotificationsState>({ items: [] });

/**
 * Shape of the {@link notificationActions} object.
 */
export interface NotificationActions {
  /** Add a new notification; returns the generated ID. */
  push(input: Omit<Notification, "id" | "createdAt">): string;
  /** Remove a notification by ID (no-op if not found). */
  dismiss(id: string): void;
  /** Clear all notifications from the store. */
  clear(): void;
}

/**
 * Generates a unique notification ID.
 * Prefers crypto.randomUUID if available, falls back to timestamp + random suffix.
 */
const newId = (): string =>
  globalThis.crypto?.randomUUID?.() ??
  `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

/**
 * Actions to mutate the notifications store.
 * Use these to add, remove, or clear toasts from the UI.
 */
export const notificationActions: NotificationActions = {
  /**
   * Add a new notification to the store.
   * Auto-dismisses after ttlMs milliseconds if specified.
   *
   * @param input - Notification data (id and createdAt are auto-assigned).
   * @returns The generated notification ID (useful for later dismissal).
   */
  push(input: Omit<Notification, "id" | "createdAt">): string {
    const id = newId();
    const item: Notification = { ...input, id, createdAt: Date.now() };
    notificationsStore.items.push(item);
    if (item.ttlMs) {
      setTimeout(() => notificationActions.dismiss(id), item.ttlMs);
    }
    return id;
  },

  /**
   * Remove a notification by ID.
   * Safe to call on non-existent IDs (no-op).
   *
   * @param id - The notification ID to dismiss.
   */
  dismiss(id: string): void {
    const items = notificationsStore.items.get();
    const idx = items.findIndex((n) => n.id === id);
    if (idx >= 0) notificationsStore.items.splice(idx, 1);
  },

  /**
   * Clear all notifications from the store.
   */
  clear(): void {
    notificationsStore.items.set([]);
  },
};
