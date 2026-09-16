import { useCallback, useEffect, useState } from "react";

/**
 * 无障碍与阅读偏好：与主题一样，只改根元素上的 data 属性，不动布局与尺寸。
 * 降动效取消粒子与背景呼吸，高对比提高文字与描边强度，并用层次几何标记区分层次。
 */
export interface Preferences {
  readonly reduceMotion: boolean;
  readonly highContrast: boolean;
}

export const DEFAULT_PREFERENCES: Preferences = {
  reduceMotion: false,
  highContrast: false,
};

const STORAGE_KEY = "thought-forge.preferences";

function readStoredPreferences(): Preferences {
  if (typeof window === "undefined") {
    return DEFAULT_PREFERENCES;
  }
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return DEFAULT_PREFERENCES;
    }
    const parsed = JSON.parse(raw) as Partial<Preferences>;
    return {
      reduceMotion: parsed.reduceMotion === true,
      highContrast: parsed.highContrast === true,
    };
  } catch {
    // 存储或内容不可用时退回默认偏好，不影响使用。
    return DEFAULT_PREFERENCES;
  }
}

export function applyPreferences(preferences: Preferences): void {
  if (typeof document === "undefined") {
    return;
  }
  const root = document.documentElement;
  root.dataset["motion"] = preferences.reduceMotion ? "reduced" : "full";
  root.dataset["contrast"] = preferences.highContrast ? "high" : "normal";
}

export function usePreferences(): readonly [
  Preferences,
  (patch: Partial<Preferences>) => void,
] {
  const [preferences, setPreferences] = useState<Preferences>(readStoredPreferences);

  useEffect(() => {
    applyPreferences(preferences);
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences));
    } catch {
      // 持久化失败不影响本次会话的偏好。
    }
  }, [preferences]);

  const update = useCallback((patch: Partial<Preferences>) => {
    setPreferences((current) => ({ ...current, ...patch }));
  }, []);

  return [preferences, update] as const;
}
