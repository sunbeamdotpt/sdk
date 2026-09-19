import { type ReactNode } from "react";
import { useSelector } from "@legendapp/state/react";
import { notificationActions, notificationsStore, type Notification } from "../state/notifications.ts";

/**
 * Render function for a single notification.
 */
export type NotificationRenderer = (props: {
  notification: Notification;
  onDismiss: (id: string) => void;
}) => ReactNode;

/**
 * Props for NotificationProvider.
 */
export interface NotificationProviderProps {
  /** Children rendered beneath the notification layer. */
  children: ReactNode;
  /**
   * Custom render function for notifications.
   * If omitted, a minimal default toast is rendered.
   */
  render?: NotificationRenderer;
}

const defaultRender: NotificationRenderer = ({ notification, onDismiss }) => (
  <div
    key={notification.id}
    style={{
      padding: "12px 16px",
      marginBottom: "8px",
      borderRadius: "6px",
      background: notification.level === "error" ? "#fee2e2" : notification.level === "warning" ? "#fef3c7" : notification.level === "success" ? "#d1fae5" : "#eff6ff",
      color: "#1f2937",
      border: `1px solid ${notification.level === "error" ? "#fca5a5" : notification.level === "warning" ? "#fcd34d" : notification.level === "success" ? "#6ee7b7" : "#93c5fd"}`,
      cursor: "pointer",
    }}
    onClick={() => onDismiss(notification.id)}
    role="alert"
  >
    <strong>{notification.title}</strong>
    {notification.body ? <p style={{ margin: "4px 0 0" }}>{notification.body}</p> : null}
  </div>
);

/**
 * Provider that renders active notifications from the notifications store.
 *
 * Place this high in your component tree (e.g. inside FrameworkProvider)
 * so that toasts overlay the entire app.
 *
 * @example
 * ```tsx
 * <FrameworkProvider transport={transport}>
 *   <NotificationProvider>
 *     <App />
 *   </NotificationProvider>
 * </FrameworkProvider>
 * ```
 */
export function NotificationProvider({
  children,
  render = defaultRender,
}: NotificationProviderProps): ReactNode {
  const items = useSelector(() => notificationsStore.items.get());

  return (
    <>
      {children}
      {items.length > 0 && (
        <div
          style={{
            position: "fixed",
            top: "16px",
            right: "16px",
            zIndex: 9999,
            maxWidth: "360px",
            display: "flex",
            flexDirection: "column",
          }}
          aria-live="polite"
          aria-atomic="false"
        >
          {items.map((n) => render({ notification: n, onDismiss: notificationActions.dismiss }))}
        </div>
      )}
    </>
  );
}
