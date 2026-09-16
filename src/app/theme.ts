import { useCallback, useEffect, useState } from "react";

export const THEME_NAMES = ["kiln", "suci"] as const;
export type ThemeName = (typeof THEME_NAMES)[number];

export const DEFAULT_THEME: ThemeName = "kiln";

const STORAGE_KEY = "thought-forge.theme";

export function isThemeName(value: unknown): value is ThemeName {
  return typeof value === "string" && (THEME_NAMES as readonly string[]).includes(value);
}

function readStoredTheme(): ThemeName {
  if (typeof window === "undefined") {
    return DEFAULT_THEME;
  }
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return isThemeName(stored) ? stored : DEFAULT_THEME;
  } catch {
    // 存储不可用时退回默认主题，不影响使用。
    return DEFAULT_THEME;
  }
}

export function applyTheme(theme: ThemeName): void {
  if (typeof document === "undefined") {
    return;
  }
  document.documentElement.dataset["theme"] = theme;
}

/**
 * 主题只改根元素的 data-theme，不改任何布局与尺寸，因此切换不重载画布。
 */
export function useTheme(): readonly [ThemeName, (next: ThemeName) => void, () => void] {
  const [theme, setTheme] = useState<ThemeName>(readStoredTheme);

  useEffect(() => {
    applyTheme(theme);
    try {
      window.localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // 持久化失败不影响本次会话的主题。
    }
  }, [theme]);

  const toggle = useCallback(() => {
    setTheme((current) => (current === "kiln" ? "suci" : "kiln"));
  }, []);

  return [theme, setTheme, toggle] as const;
}
