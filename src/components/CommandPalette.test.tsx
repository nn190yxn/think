import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { CommandPalette, parseIntent, type PaletteAction } from "./CommandPalette";
import { DEFAULT_PREFERENCES } from "../app/preferences";

function makeAction(id: string, title: string, keywords: string[]) {
  return { id, title, hint: "", keywords, run: () => undefined };
}

const actions: readonly PaletteAction[] = [
  makeAction("go:observe", "前往思维星图", ["观", "星图"]),
  makeAction("theme:toggle", "切换到素瓷（明色）", ["主题", "主题切换"]),
  makeAction("a11y:contrast", "开启高对比模式", ["对比"]),
];

describe("命令面板意图解析", () => {
  it("空输入返回全部命令", () => {
    expect(parseIntent("", actions)).toHaveLength(3);
  });

  it("结构化指令按前缀匹配命令 id", () => {
    const result = parseIntent("/go", actions);
    expect(result.map((action) => action.id)).toEqual(["go:observe"]);
  });

  it("自然语言按标题与关键词匹配", () => {
    expect(parseIntent("对比", actions).map((action) => action.id)).toEqual([
      "a11y:contrast",
    ]);
  });
});

describe("命令面板", () => {
  function renderPalette(overrides: Partial<Parameters<typeof CommandPalette>[0]> = {}) {
    const onClose = vi.fn();
    const onNavigate = vi.fn();
    const onSeedCouncil = vi.fn();
    render(
      <CommandPalette
        open
        onClose={onClose}
        onNavigate={onNavigate}
        onSeedCouncil={onSeedCouncil}
        theme="kiln"
        onThemeChange={() => undefined}
        preferences={DEFAULT_PREFERENCES}
        onPreferencesChange={() => undefined}
        {...overrides}
      />,
    );
    return { onClose, onNavigate, onSeedCouncil };
  }

  it("列出可执行命令并可用键盘执行", async () => {
    const { onNavigate, onClose } = renderPalette();
    const input = screen.getByRole("combobox");
    expect(await screen.findByRole("option", { name: /前往思维星图/ })).toBeInTheDocument();

    await userEvent.click(input);
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(onNavigate).toHaveBeenCalledWith("council");
    expect(onClose).toHaveBeenCalled();
  });

  it("自然语言问句可转为会诊议题", async () => {
    const { onSeedCouncil } = renderPalette();
    await userEvent.type(screen.getByRole("combobox"), "要不要换工作？");
    const ask = await screen.findByRole("option", { name: /发起会诊/ });
    await userEvent.click(ask);
    expect(onSeedCouncil).toHaveBeenCalledWith("要不要换工作？");
  });

  it("Esc 关闭面板", async () => {
    const { onClose } = renderPalette();
    await userEvent.type(screen.getByRole("combobox"), "{Escape}");
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });
});
