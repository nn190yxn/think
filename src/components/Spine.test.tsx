import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { defaultSpineEntries, Spine } from "./Spine";

describe("炉脊", () => {
  it("五个境界各有一个印记", () => {
    render(<Spine entries={defaultSpineEntries()} current="observe" onSelect={() => {}} />);
    const nav = screen.getByRole("navigation", { name: "境界" });
    expect(nav.querySelectorAll(".spine__item")).toHaveLength(5);
  });

  it("当前境界标记为 page", () => {
    render(<Spine entries={defaultSpineEntries()} current="refine" onSelect={() => {}} />);
    const current = screen.getByRole("button", { name: /蒸馏熔炉/ });
    expect(current).toHaveAttribute("aria-current", "page");
  });

  it("点击印记切换境界", async () => {
    const onSelect = vi.fn();
    render(<Spine entries={defaultSpineEntries()} current="observe" onSelect={onSelect} />);
    await userEvent.click(screen.getByRole("button", { name: /圆桌会诊/ }));
    expect(onSelect).toHaveBeenCalledWith("council");
  });

  it("有新内容的境界带可读提示，不依赖颜色", () => {
    render(<Spine entries={defaultSpineEntries()} current="observe" onSelect={() => {}} />);
    const active = screen.getByRole("button", { name: /思维星图/ });
    expect(active).toHaveAccessibleName(/有新内容/);
  });

  it("活跃度以数值形式传给光带，便于绘制", () => {
    render(<Spine entries={defaultSpineEntries()} current="observe" onSelect={() => {}} />);
    const ribbon = document.querySelector(".spine__ribbon");
    expect(ribbon).not.toBeNull();
    expect(ribbon?.getAttribute("style")).toContain("--activity");
  });
});
