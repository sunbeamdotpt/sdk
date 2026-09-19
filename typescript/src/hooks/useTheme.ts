import { useSelector } from "@legendapp/state/react";
import { uiActions, uiStore, type Theme } from "../state/ui.ts";

/**
 * Hook to access and control the application theme.
 * Provides reactive access to the current resolved theme ("dark" or "light")
 * and functions to change it. The "system" preference is resolved automatically
 * via `prefers-color-scheme` and synced to `document.documentElement.dataset.theme`
 * so that `@sunbeam/beam-ui` picks it up.
 *
 * @returns Object containing:
 *   - theme: The resolved theme value ("dark" or "light"). Never "system".
 *   - rawTheme: The stored preference ("light" | "dark" | "system").
 *   - setTheme: Function to set the theme preference.
 *   - toggle: Function to toggle between "dark" and "light".
 *
 * @example
 * ```tsx
 * export function ThemeToggle() {
 *   const { theme, toggle } = useTheme();
 *
 *   return (
 *     <button onClick={toggle}>
 *       Switch to {theme === "dark" ? "light" : "dark"} mode
 *     </button>
 *   );
 * }
 * ```
 */
export function useTheme(): {
  theme: "light" | "dark";
  rawTheme: Theme;
  setTheme: (t: Theme) => void;
  toggle: () => void;
} {
  const rawTheme = useSelector(() => uiStore.theme.get());

  const resolved: "light" | "dark" =
    rawTheme !== "system"
      ? rawTheme
      : typeof window !== "undefined" &&
          window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";

  return {
    theme: resolved,
    rawTheme,
    setTheme: uiActions.setTheme,
    toggle: () => uiActions.setTheme(resolved === "dark" ? "light" : "dark"),
  };
}
