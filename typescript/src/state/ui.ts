import { observable, type Observable } from "@legendapp/state";

/**
 * Color theme preference.
 * "system" respects the OS dark mode setting.
 */
export type Theme = "light" | "dark" | "system";

/**
 * Spacing and sizing density for UI components.
 * Determines padding, margins, and icon sizes globally.
 */
export type Density = "comfortable" | "compact";

/**
 * State object tracking user UI preferences (theme, density, layout, locale).
 * Automatically persisted to localStorage and restored on page load.
 */
export interface UiState {
  /** Color theme preference. */
  theme: Theme;
  /** Spacing density preference. */
  density: Density;
  /** Whether the sidebar is collapsed (true = collapsed, false = expanded). */
  sidebarCollapsed: boolean;
  /** BCP 47 language tag (e.g., "en-US", "de", "zh-CN") for i18n. */
  locale: string;
}

const STORAGE_KEY = "sunbeam-g2v:ui";

const defaults: UiState = {
  theme: "system",
  density: "comfortable",
  sidebarCollapsed: false,
  locale: typeof navigator !== "undefined" ? navigator.language : "en-US",
};

/**
 * Restores UI state from localStorage, merging with defaults.
 * Returns defaults if storage is unavailable or corrupt.
 */
const restore = (): UiState => {
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (!raw) return defaults;
    return { ...defaults, ...(JSON.parse(raw) as Partial<UiState>) };
  } catch {
    return defaults;
  }
};

/**
 * Observable store tracking user UI preferences (theme, density, sidebar, locale).
 * Automatically persists to localStorage on every change. Subscribe to react to
 * theme/density/layout changes. Restored from storage on page load.
 *
 * @example
 * ```tsx
 * const theme = uiStore.theme.get();
 * uiActions.setTheme("dark");
 * uiActions.toggleSidebar();
 * ```
 */
export const uiStore: Observable<UiState> = observable<UiState>(restore());

const resolveTheme = (theme: Theme): "light" | "dark" => {
  if (theme !== "system") return theme;
  if (typeof window === "undefined") return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
};

uiStore.onChange(({ value }) => {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    /* storage unavailable */
  }
  if (typeof document !== "undefined") {
    document.documentElement.dataset.theme = resolveTheme(value.theme);
  }
});

/**
 * Actions to mutate UI preferences.
 * Use these to respond to user setting changes. All mutations are persisted
 * to localStorage automatically.
 */
/**
 * Shape of the {@link uiActions} object.
 */
export interface UiActions {
  /** Set the color theme preference ("light" | "dark" | "system"). */
  setTheme(theme: Theme): void;
  /** Set the spacing density preference ("comfortable" | "compact"). */
  setDensity(density: Density): void;
  /** Toggle the sidebar between collapsed and expanded. */
  toggleSidebar(): void;
  /** Set the UI locale (BCP 47 language tag). */
  setLocale(locale: string): void;
}

/**
 * Actions to mutate UI preferences.
 * Use these to respond to user setting changes. All mutations are persisted
 * to localStorage automatically.
 */
export const uiActions: UiActions = {
  /**
   * Set the color theme preference.
   *
   * @param theme - The theme to apply ("light" | "dark" | "system").
   */
  setTheme(theme: Theme): void {
    uiStore.theme.set(theme);
  },

  /**
   * Set the spacing density preference.
   *
   * @param density - The density level ("comfortable" | "compact").
   */
  setDensity(density: Density): void {
    uiStore.density.set(density);
  },

  /**
   * Toggle the sidebar between collapsed and expanded.
   */
  toggleSidebar(): void {
    uiStore.sidebarCollapsed.set((v) => !v);
  },

  /**
   * Set the UI locale for internationalization.
   *
   * @param locale - BCP 47 language tag (e.g., "en-US", "de", "zh-CN").
   */
  setLocale(locale: string): void {
    uiStore.locale.set(locale);
  },
};
