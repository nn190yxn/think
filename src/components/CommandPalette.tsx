import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { REALM_KEYS, realmOf, type RealmKey } from "../domain/realms";
import type { ThemeName } from "../app/theme";
import type { Preferences } from "../app/preferences";

export interface PaletteAction {
  readonly id: string;
  readonly title: string;
  readonly hint: string;
  readonly keywords: readonly string[];
  readonly run: () => void;
}

export interface CommandPaletteProps {
  readonly open: boolean;
  readonly onClose: () => void;
  readonly onNavigate: (realm: RealmKey) => void;
  readonly onSeedCouncil?: ((question: string) => void) | undefined;
  readonly theme: ThemeName;
  readonly onThemeChange: (next: ThemeName) => void;
  readonly preferences: Preferences;
  readonly onPreferencesChange: (patch: Partial<Preferences>) => void;
}

const REALM_KEYWORDS: Record<RealmKey, readonly string[]> = {
  observe: ["观", "星图", "网络", "observe"],
  council: ["会诊", "圆桌", "council"],
  refine: ["炼", "蒸馏", "refine"],
  vault: ["藏", "大师", "资产", "知识", "vault"],
  self: ["我", "成长", "设置", "自我画像", "self"],
};

/**
 * 把自然语言收成一组候选动作。没有模型也能用：先用关键词命中结构化指令，
 * 命中不到就把整句当成会诊议题。结构化输入以「/」开头，只做前缀匹配。
 */
export function parseIntent(
  input: string,
  actions: readonly PaletteAction[],
): readonly PaletteAction[] {
  const text = input.trim();
  if (!text) {
    return actions;
  }
  const lower = text.toLowerCase();
  const structured = text.startsWith("/");
  const needle = structured ? lower.slice(1) : lower;

  const matched = actions.filter((action) => {
    if (structured) {
      return (
        action.id.toLowerCase().startsWith(needle) ||
        action.keywords.some((word) => word.startsWith(needle))
      );
    }
    return (
      action.title.toLowerCase().includes(needle) ||
      action.keywords.some((word) => word.toLowerCase().includes(needle))
    );
  });

  return matched;
}

export function CommandPalette({
  open,
  onClose,
  onNavigate,
  onSeedCouncil,
  theme,
  onThemeChange,
  preferences,
  onPreferencesChange,
}: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const baseActions = useMemo<readonly PaletteAction[]>(() => {
    const navigate = REALM_KEYS.map((key) => ({
      id: `go:${key}`,
      title: `前往${realmOf(key).title}`,
      hint: realmOf(key).sigil,
      keywords: REALM_KEYWORDS[key],
      run: () => onNavigate(key),
    }));
    const appearance: PaletteAction[] = [
      {
        id: "theme:toggle",
        title: theme === "kiln" ? "切换到素瓷（明色）" : "切换到窑变（暗色）",
        hint: "主题",
        keywords: ["主题", "明色", "暗色", "素瓷", "窑变", "theme"],
        run: () => onThemeChange(theme === "kiln" ? "suci" : "kiln"),
      },
      {
        id: "a11y:motion",
        title: preferences.reduceMotion ? "关闭降低动态效果" : "开启降低动态效果",
        hint: "无障碍",
        keywords: ["动效", "动画", "动态", "motion"],
        run: () => onPreferencesChange({ reduceMotion: !preferences.reduceMotion }),
      },
      {
        id: "a11y:contrast",
        title: preferences.highContrast ? "关闭高对比模式" : "开启高对比模式",
        hint: "无障碍",
        keywords: ["对比", "contrast"],
        run: () => onPreferencesChange({ highContrast: !preferences.highContrast }),
      },
    ];
    return [...navigate, ...appearance];
  }, [onNavigate, onPreferencesChange, onThemeChange, preferences, theme]);

  const actions = useMemo(() => {
    const trimmed = query.trim();
    const shouldAsk =
      trimmed.length > 0 &&
      !trimmed.startsWith("/") &&
      onSeedCouncil !== undefined &&
      /[?？]$|会诊|问问|怎么|如何|应不应该|要不要/.test(trimmed);
    const ask: PaletteAction[] = shouldAsk
      ? [
          {
            id: "council:ask",
            title: `以「${trimmed}」发起会诊`,
            hint: "圆桌",
            keywords: [],
            run: () => onSeedCouncil(trimmed),
          },
        ]
      : [];
    return [...ask, ...parseIntent(query, baseActions)];
  }, [baseActions, onSeedCouncil, query]);

  useEffect(() => {
    if (open) {
      setQuery("");
      setActive(0);
      inputRef.current?.focus();
    }
  }, [open]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  if (!open) {
    return null;
  }

  function run(action: PaletteAction | undefined) {
    if (!action) {
      return;
    }
    action.run();
    onClose();
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((current) => (actions.length === 0 ? 0 : (current + 1) % actions.length));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((current) =>
        actions.length === 0 ? 0 : (current - 1 + actions.length) % actions.length,
      );
    } else if (event.key === "Enter") {
      event.preventDefault();
      run(actions[active]);
    }
  }

  return (
    <div className="palette">
      <button
        className="palette__scrim"
        type="button"
        aria-label="关闭命令面板"
        onClick={onClose}
      />
      <div
        className="palette__panel"
        role="dialog"
        aria-modal="true"
        aria-label="命令面板"
      >
        <input
          ref={inputRef}
          className="palette__input"
          type="text"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-autocomplete="list"
          aria-activedescendant={actions[active] ? `palette-${actions[active]!.id}` : undefined}
          placeholder="直接说你想做什么，或输入 / 看可选动作"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onKeyDown}
        />
        <ul className="palette__list" id="palette-list" role="listbox" aria-label="候选命令">
          {actions.length === 0 ? (
            <li className="palette__empty">
              没有匹配的项，试试「会诊」「主题」，或者直接写一句话发起会诊
            </li>
          ) : (
            actions.map((action, index) => (
              <li
                key={action.id}
                id={`palette-${action.id}`}
                className="palette__item"
                role="option"
                aria-selected={index === active}
                onMouseEnter={() => setActive(index)}
                onClick={() => run(action)}
              >
                <span className="palette__title">{action.title}</span>
                <span className="palette__hint mono">{action.hint}</span>
              </li>
            ))
          )}
        </ul>
        <p className="palette__foot mono">↑↓ 选择 · Enter 执行 · Esc 关闭</p>
      </div>
    </div>
  );
}
