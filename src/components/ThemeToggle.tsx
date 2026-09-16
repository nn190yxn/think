import { THEME_NAMES, type ThemeName } from "../app/theme";

const LABELS: Record<ThemeName, string> = {
  kiln: "窑变",
  suci: "素瓷",
};

export function ThemeToggle({
  theme,
  onChange,
}: {
  readonly theme: ThemeName;
  readonly onChange: (next: ThemeName) => void;
}) {
  return (
    <div className="theme-toggle" role="group" aria-label="主题">
      {THEME_NAMES.map((name) => (
        <button
          key={name}
          type="button"
          className="theme-toggle__item"
          aria-pressed={theme === name}
          data-active={theme === name}
          onClick={() => onChange(name)}
        >
          {LABELS[name]}
        </button>
      ))}
    </div>
  );
}
