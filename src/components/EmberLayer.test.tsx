import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { EmberLayer, emberFade } from "./EmberLayer";
import { IpcProvider } from "../app/ipc";
import type { CouncilSessionView } from "../ipc/commands";

function renderEmber(onOpenCouncil?: (session: CouncilSessionView) => void) {
  return render(
    <IpcProvider>
      <EmberLayer onOpenCouncil={onOpenCouncil} />
    </IpcProvider>,
  );
}

describe("余烬", () => {
  it("未处理的发现聚成一簇余烬，点开后按性质分色漂浮", async () => {
    renderEmber();
    const cluster = await screen.findByRole("button", { name: /余烬，\d+ 条待看的发现/ });
    await userEvent.click(cluster);

    const tray = await screen.findByRole("group", { name: "待看的发现" });
    expect(tray.querySelectorAll("li")).toHaveLength(3);
    expect(screen.getByText("关联")).toBeInTheDocument();
    expect(screen.getByText("冲突")).toBeInTheDocument();
    expect(screen.getByText("盲区")).toBeInTheDocument();
  });

  it("采纳后卡片离场", async () => {
    renderEmber();
    await userEvent.click(
      await screen.findByRole("button", { name: /余烬，\d+ 条待看的发现/ }),
    );
    const title = "两次「先扩张」的判断其实同源";
    const card = (await screen.findByText(title)).closest("li");
    expect(card).not.toBeNull();

    await userEvent.click(within(card as HTMLElement).getByRole("button", { name: "采纳" }));
    await waitFor(() => expect(screen.queryByText(title)).not.toBeInTheDocument());
  });

  it("拿去会诊把发现送入圆桌", async () => {
    const onOpenCouncil = vi.fn();
    renderEmber(onOpenCouncil);
    await userEvent.click(
      await screen.findByRole("button", { name: /余烬，\d+ 条待看的发现/ }),
    );
    await screen.findAllByRole("button", { name: "拿去会诊" });

    await userEvent.click(screen.getAllByRole("button", { name: "拿去会诊" })[0]!);
    await waitFor(() => expect(onOpenCouncil).toHaveBeenCalledTimes(1));
    expect(onOpenCouncil.mock.calls[0]?.[0]).toMatchObject({ id: expect.any(String) });
  });
});

describe("余烬亮度衰减", () => {
  it("越旧越暗，但保留三成下限", () => {
    const now = Date.parse("2026-09-14T12:00:00Z");
    expect(emberFade("2026-09-14T12:00:00Z", now)).toBeCloseTo(1, 5);
    expect(emberFade("2026-09-13T12:00:00Z", now)).toBeLessThan(1);
    expect(emberFade("2026-01-01T00:00:00Z", now)).toBe(0.3);
    expect(emberFade("不是时间", now)).toBe(1);
  });
});
