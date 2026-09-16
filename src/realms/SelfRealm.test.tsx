import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { useState } from "react";
import { SelfRealm } from "./SelfRealm";
import { IpcProvider } from "../app/ipc";
import { DEMO_QUESTION } from "../ipc/demoData";
import { DEFAULT_PREFERENCES, type Preferences } from "../app/preferences";

function RealmHarness() {
  const [preferences, setPreferences] = useState<Preferences>(DEFAULT_PREFERENCES);
  return (
    <SelfRealm
      theme="kiln"
      onThemeChange={() => undefined}
      preferences={preferences}
      onPreferencesChange={(patch) =>
        setPreferences((current) => ({ ...current, ...patch }))
      }
    />
  );
}

function renderRealm() {
  return render(
    <IpcProvider>
      <RealmHarness />
    </IpcProvider>,
  );
}

describe("我境界 · 成长轨迹", () => {
  it("列出思考记录与演化链，采纳状态可见", async () => {
    renderRealm();
    expect(await screen.findByText("成长轨迹")).toBeInTheDocument();
    const questions = await screen.findAllByText(DEMO_QUESTION);
    expect(questions).toHaveLength(2);
    expect(screen.getByRole("button", { name: "已采纳" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    await userEvent.click(screen.getAllByRole("button", { name: "演化链" })[0]!);
    const chain = await screen.findByRole("list", { name: "演化链" });
    expect(chain.querySelectorAll("li")).toHaveLength(2);
  });

  it("固化面板展示本次结果与历史", async () => {
    renderRealm();
    expect(await screen.findByText("记忆固化")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "立即固化" }));
    await waitFor(() => expect(screen.getByText("强化连线")).toBeInTheDocument());
    expect(screen.getByText("识别冲突")).toBeInTheDocument();
    expect(screen.getAllByText("manual").length).toBeGreaterThan(0);
  });

  it("调参面板按组展示范围，越界提示且整批不生效", async () => {
    renderRealm();
    expect(await screen.findByText("调参")).toBeInTheDocument();
    const input = await screen.findByLabelText("质询轮次上限");
    expect(input).toHaveValue("3");

    await userEvent.clear(input);
    await userEvent.type(input, "9");
    expect(await screen.findByText("取值超出允许范围")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存全部" })).toBeDisabled();

    await userEvent.clear(input);
    await userEvent.type(input, "4");
    await userEvent.click(screen.getByRole("button", { name: "保存全部" }));
    await waitFor(() => expect(screen.getByText("已保存")).toBeInTheDocument());
    expect(screen.getByLabelText("质询轮次上限")).toHaveValue("4");
  });

  it("主动助学默认关闭，可开启并调整每日上限", async () => {
    renderRealm();
    const toggle = await screen.findByRole("switch", { name: "主动助学" });
    expect(toggle).toHaveAttribute("aria-checked", "false");

    await userEvent.click(toggle);
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "主动助学" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "10 条" }));
    await waitFor(() => expect(screen.getByText("0 / 10")).toBeInTheDocument());
  });

  it("议题长河可展开演化链，原则与年轮概览可见", async () => {
    renderRealm();
    expect(await screen.findByText("演化长河")).toBeInTheDocument();
    expect(screen.getByText("个人原则")).toBeInTheDocument();
    expect(screen.getByText("年轮概览")).toBeInTheDocument();

    await userEvent.click(screen.getAllByRole("button", { name: "看演化链" })[0]!);
    const chain = await screen.findByRole("list", { name: "演化链" });
    expect(chain.querySelectorAll("li").length).toBeGreaterThan(0);

    await userEvent.click(screen.getByRole("button", { name: "沉淀原则" }));
    await waitFor(() =>
      expect(screen.getByText("还没有连续采纳三次的议题")).toBeInTheDocument(),
    );
  });

  it("采集台可暂停、恢复并删除记录，系统不可用的能力被禁用", async () => {
    renderRealm();
    expect(await screen.findByText("采集台")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "脱敏" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByRole("switch", { name: "文件活动" })).toBeDisabled();

    await userEvent.click(screen.getByRole("switch", { name: "全局暂停" }));
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "全局暂停" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "立即采集" }));
    await waitFor(() =>
      expect(screen.getByText("已暂停，本轮未采集")).toBeInTheDocument(),
    );

    await userEvent.click(screen.getByRole("switch", { name: "全局暂停" }));
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "全局暂停" })).toHaveAttribute(
        "aria-checked",
        "false",
      ),
    );

    const before = screen.getAllByRole("button", { name: "删除" }).length;
    expect(before).toBeGreaterThan(0);
    await userEvent.click(screen.getAllByRole("button", { name: "删除" })[0]!);
    await waitFor(() =>
      expect(screen.getAllByRole("button", { name: "删除" })).toHaveLength(before - 1),
    );
  });

  it("添加关注目录后文件活动可开启，移除后重新不可用", async () => {
    renderRealm();
    expect(await screen.findByText("采集台")).toBeInTheDocument();

    // 未设置关注目录时，文件活动没有可用来源。
    expect(screen.getByText("还没有关注目录，文件活动因此不可用。")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "文件活动" })).toBeDisabled();

    await userEvent.type(
      screen.getByRole("textbox", { name: "新增关注目录" }),
      "/tmp/notes",
    );
    await userEvent.click(screen.getByRole("button", { name: "添加" }));

    const roots = await screen.findByRole("list", { name: "关注目录" });
    expect(within(roots).getByText("/tmp/notes")).toBeInTheDocument();
    expect(screen.getByText("已监听 1 个目录")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "文件活动" })).toBeEnabled(),
    );

    await userEvent.click(
      within(roots).getByRole("button", { name: "移除关注目录 /tmp/notes" }),
    );
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "文件活动" })).toBeDisabled(),
    );
    expect(screen.getByText("已清空关注目录")).toBeInTheDocument();
  });

  it("铜镜展示解锁进度，逐条确认后可安装为会诊席位", async () => {
    renderRealm();
    expect(await screen.findByText("铜镜 · 自我蒸馏")).toBeInTheDocument();
    expect(screen.getByText("24 / 20")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "安装并加入会诊" })).toBeDisabled();

    const list = await screen.findByRole("list", { name: "自我蒸馏初稿" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(5);

    await userEvent.click(
      within(within(list).getAllByRole("listitem")[0]!).getByRole("button", {
        name: "采纳",
      }),
    );
    await waitFor(() =>
      expect(within(list).getAllByText("已采纳").length).toBeGreaterThan(0),
    );
    // 仍有待确认条目，安装按钮保持禁用。
    expect(screen.getByRole("button", { name: "安装并加入会诊" })).toBeDisabled();

    for (const item of within(list).getAllByRole("listitem").slice(1)) {
      await userEvent.click(within(item).getByRole("button", { name: "采纳" }));
    }
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "安装并加入会诊" })).toBeEnabled(),
    );

    await userEvent.click(screen.getByRole("button", { name: "安装并加入会诊" }));
    const seat = await screen.findByRole("switch", { name: "自我席位" });
    expect(seat).toHaveAttribute("aria-checked", "true");

    await userEvent.click(seat);
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "自我席位" })).toHaveAttribute(
        "aria-checked",
        "false",
      ),
    );
  });

  it("数据主权展示范围，导出只读、清除需二次确认并留痕", async () => {
    renderRealm();
    expect(await screen.findByText("数据主权")).toBeInTheDocument();
    expect(screen.getByText("10 张表 · 1482 行")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "导出为 JSON" }));
    expect(await screen.findByText(/已导出 1482 行到/)).toBeInTheDocument();
    // 导出是只读操作，不触发清除。
    expect(dataPurgeButton()).toHaveAttribute("aria-pressed", "false");

    await userEvent.click(dataPurgeButton());
    await waitFor(() =>
      expect(dataPurgeButton()).toHaveAttribute("aria-pressed", "true"),
    );
    await userEvent.click(screen.getByRole("button", { name: "确认清除，不可撤销" }));
    expect(await screen.findByText(/已清除 1482 行/)).toBeInTheDocument();
  });

  it("连接器面板可开关、测试连通并展示调用审计", async () => {
    renderRealm();
    const heading = await screen.findByRole("heading", { name: "连接器" });
    const panel = heading.closest(".panel") as HTMLElement;
    expect(within(panel).getAllByRole("switch")).toHaveLength(3);

    const search = within(panel).getByRole("switch", { name: "本地检索（占位）" });
    expect(search).toHaveAttribute("aria-checked", "true");
    await userEvent.click(search);
    await waitFor(() =>
      expect(
        within(panel).getByRole("switch", { name: "本地检索（占位）" }),
      ).toHaveAttribute("aria-checked", "false"),
    );

    await userEvent.click(
      within(panel).getAllByRole("button", { name: "连通测试" })[0]!,
    );
    const preflight = await screen.findByRole("group", { name: "发送前确认" });
    expect(within(preflight).getByText("实际发送")).toBeInTheDocument();
    await userEvent.click(within(preflight).getByRole("button", { name: "确认发送" }));

    expect(await screen.findByText(/连通测试(通过|失败)/)).toBeInTheDocument();
    expect(within(panel).getAllByText("共享背景检索").length).toBeGreaterThan(0);
    expect(within(panel).getByText("席位补充检索")).toBeInTheDocument();
  });

  it("外观面板可开关降低动态效果与高对比模式", async () => {
    renderRealm();
    const motion = await screen.findByRole("switch", { name: "降低动态效果" });
    expect(motion).toHaveAttribute("aria-checked", "false");
    await userEvent.click(motion);
    await waitFor(() => expect(motion).toHaveAttribute("aria-checked", "true"));

    const contrast = screen.getByRole("switch", { name: "高对比模式" });
    await userEvent.click(contrast);
    await waitFor(() => expect(contrast).toHaveAttribute("aria-checked", "true"));
  });

  it("成本面板展示估算与日月累计，凭据只写引用，备份可创建并恢复", async () => {
    renderRealm();
    const heading = await screen.findByRole("heading", { name: "成本与配额" });
    const panel = heading.closest(".panel") as HTMLElement;

    expect(within(panel).getByText("单场估算")).toBeInTheDocument();
    expect(within(panel).getByText("今日累计")).toBeInTheDocument();
    expect(within(panel).getByText("本月累计")).toBeInTheDocument();
    expect(within(panel).getByText("超限策略")).toBeInTheDocument();

    await userEvent.type(
      within(panel).getByLabelText("归属"),
      "cloud",
    );
    await userEvent.type(
      within(panel).getByLabelText("密钥"),
      "sk-demo-secret",
    );
    await userEvent.click(
      within(panel).getByRole("button", { name: "写入凭据库" }),
    );
    expect(
      await within(panel).findByText("密钥已写入系统凭据库，数据库只保留引用名"),
    ).toBeInTheDocument();
    expect(within(panel).getByText("已配置")).toBeInTheDocument();

    await userEvent.click(within(panel).getByRole("button", { name: "立即备份" }));
    expect(await within(panel).findByText(/已创建备份/)).toBeInTheDocument();

    await userEvent.click(
      within(panel).getAllByRole("button", { name: "校验并恢复" })[0]!,
    );
    expect(
      await within(panel).findByText("备份已校验并恢复，重启应用后生效"),
    ).toBeInTheDocument();
  });
});

function dataPurgeButton() {
  return screen.getByRole("button", { name: /清除全部数据|确认清除，不可撤销/ });
}
