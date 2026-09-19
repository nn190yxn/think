import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { useState } from "react";
import { SelfRealm } from "./SelfRealm";
import { IpcProvider } from "../app/ipc";
import { createCommandClient, stubTransport } from "../ipc/client";
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

/** 设置视图放着体检、用量、外观等低频项目，进入前先切到「设置」页。 */
async function showSystem() {
  await userEvent.click(await screen.findByRole("tab", { name: "设置" }));
}

describe("我境界 · 成长轨迹", () => {
  it("成长与系统分两页，系统设置只在系统页可达", async () => {
    renderRealm();
    expect(screen.getByRole("tab", { name: "成长" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    // 成长页看不到低频的系统设置。
    expect(await screen.findByRole("switch", { name: "主动助学" })).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "联网能力" })).toBeNull();

    await showSystem();
    expect(await screen.findByRole("switch", { name: "联网能力" })).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "主动助学" })).toBeNull();
  });

  it("设置页给出分区导航，每一节都指向真实存在的面板", async () => {
    renderRealm();
    // 成长页不放分区导航。
    expect(screen.queryByRole("navigation", { name: "设置分区" })).toBeNull();

    await showSystem();
    const nav = await screen.findByRole("navigation", { name: "设置分区" });
    const items = within(nav).getAllByRole("button");
    expect(items).toHaveLength(10);

    for (const item of items) {
      const target = item.getAttribute("aria-controls") ?? "";
      // 面板改了标题却忘了同步导航，会在这一句暴露。
      expect(document.getElementById(target)).not.toBeNull();
    }

    // 点一下不报错，也不把界面切走。
    await userEvent.click(items[5]!);
    expect(within(nav).getAllByRole("button")[5]).toHaveTextContent("调参");
  });

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
    await waitFor(() =>
      expect(screen.getByText("加深了的连接")).toBeInTheDocument(),
    );
    expect(screen.getByText("发现的矛盾")).toBeInTheDocument();
    expect(screen.getAllByText("手动触发").length).toBeGreaterThan(0);
  });

  it("调参面板按组展示范围，越界提示且整批不生效", async () => {
    renderRealm();
    await showSystem();
    // 分区导航也有「调参」两个字，这里认准面板标题。
    expect(await screen.findByRole("heading", { name: "调参" })).toBeInTheDocument();
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
    expect(screen.getByRole("switch", { name: "遮蔽敏感信息" })).toHaveAttribute(
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

  it("自我画像展示解锁进度，逐条确认后可安装为会诊席位", async () => {
    renderRealm();
    expect(await screen.findByText("自我画像")).toBeInTheDocument();
    expect(screen.getByText("24 / 20")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "安装并加入会诊" })).toBeDisabled();

    const list = await screen.findByRole("list", { name: "自我画像初稿" });
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
    const seat = await screen.findByRole("switch", { name: "把你自己加入会诊" });
    expect(seat).toHaveAttribute("aria-checked", "true");

    await userEvent.click(seat);
    await waitFor(() =>
      expect(
        screen.getByRole("switch", { name: "把你自己加入会诊" }),
      ).toHaveAttribute("aria-checked", "false"),
    );
  });

  it("数据主权展示范围，导出只读、清除需二次确认并留痕", async () => {
    renderRealm();
    await showSystem();
    expect(await screen.findByRole("heading", { name: "数据主权" })).toBeInTheDocument();
    expect(screen.getByText("共 1482 条记录")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "导出数据文件" }));
    expect((await screen.findAllByText(/已导出 1482 条记录/)).length).toBeGreaterThan(0);
    // 导出是只读操作，不触发清除。
    expect(dataPurgeButton()).toHaveAttribute("aria-pressed", "false");

    await userEvent.click(dataPurgeButton());
    await waitFor(() =>
      expect(dataPurgeButton()).toHaveAttribute("aria-pressed", "true"),
    );
    await userEvent.click(screen.getByRole("button", { name: "确认清除，不可撤销" }));
    expect(await screen.findByText(/已清除 1482 条记录/)).toBeInTheDocument();
  });

  it("外部数据源面板可开关、测试连接并展示调用记录", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "外部数据源" });
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
      within(panel).getAllByRole("button", { name: "测试连接" })[0]!,
    );
    const preflight = await screen.findByRole("group", { name: "发送前确认" });
    expect(within(preflight).getByText("实际发送")).toBeInTheDocument();
    await userEvent.click(within(preflight).getByRole("button", { name: "确认发送" }));

    expect(await screen.findByText(/已连上|没能连上/)).toBeInTheDocument();
    expect(
      within(panel).getAllByText("会诊前搜集共享背景").length,
    ).toBeGreaterThan(0);
    expect(within(panel).getByText("参与者自行补充资料")).toBeInTheDocument();
  });

  it("外观面板可开关降低动态效果与高对比模式", async () => {
    renderRealm();
    await showSystem();
    const motion = await screen.findByRole("switch", { name: "降低动态效果" });
    expect(motion).toHaveAttribute("aria-checked", "false");
    await userEvent.click(motion);
    await waitFor(() => expect(motion).toHaveAttribute("aria-checked", "true"));

    const contrast = screen.getByRole("switch", { name: "高对比模式" });
    await userEvent.click(contrast);
    await waitFor(() => expect(contrast).toHaveAttribute("aria-checked", "true"));
  });

  it("用量面板展示估算与日月累计", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "用量" });
    const panel = heading.closest(".panel") as HTMLElement;

    expect(within(panel).getByText("单场估算")).toBeInTheDocument();
    expect(within(panel).getByText("今日累计")).toBeInTheDocument();
    expect(within(panel).getByText("本月累计")).toBeInTheDocument();
    expect(within(panel).getByText("超限策略")).toBeInTheDocument();
  });

  it("备份与恢复独立成面板，可创建并恢复", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "备份与恢复" });
    const panel = heading.closest(".panel") as HTMLElement;

    await userEvent.click(within(panel).getByRole("button", { name: "立即备份" }));
    expect(await within(panel).findByText(/已创建备份/)).toBeInTheDocument();

    await userEvent.click(
      within(panel).getAllByRole("button", { name: "校验并恢复" })[0]!,
    );
    expect(
      await within(panel).findByText("备份已校验并恢复，重启应用后生效"),
    ).toBeInTheDocument();
  });

  it("密钥写在平台面板下，只留条目引用", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "联网与模型平台" });
    const panel = heading.closest(".panel") as HTMLElement;

    await userEvent.type(within(panel).getByLabelText("归属"), "cloud");
    await userEvent.type(within(panel).getByLabelText("密钥"), "sk-demo-secret");
    await userEvent.click(within(panel).getByRole("button", { name: "保存密钥" }));
    expect(
      await within(panel).findByText("密钥已存进系统密钥库，本应用只记住它的名字"),
    ).toBeInTheDocument();
    expect(within(panel).getByText("已配置")).toBeInTheDocument();
  });

  it("平台表单可新增平台，并说明密钥条目名", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "联网与模型平台" });
    const panel = heading.closest(".panel") as HTMLElement;

    await userEvent.type(within(panel).getByLabelText("平台代码"), "deepseek");
    await userEvent.type(within(panel).getByLabelText("显示名"), "DeepSeek");
    await userEvent.type(
      within(panel).getByLabelText("服务地址"),
      "https://api.deepseek.com/chat/completions",
    );
    await userEvent.type(within(panel).getByLabelText("模型名"), "deepseek-chat");
    // 单价按元填写，内核按百万分之一元记账。
    await userEvent.type(within(panel).getByLabelText("输入单价"), "0.002");
    await userEvent.click(within(panel).getByRole("button", { name: "保存平台" }));

    expect(
      await within(panel).findByText(
        "已保存 DeepSeek。密钥按代码 deepseek 写进下面的「密钥」里。",
      ),
    ).toBeInTheDocument();
    // 新平台立刻进列表，不必等下次刷新。
    expect(within(panel).getByText("DeepSeek")).toBeInTheDocument();
  });

  it("平台表单缺服务地址或模型名时不提交", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "联网与模型平台" });
    const panel = heading.closest(".panel") as HTMLElement;

    await userEvent.type(within(panel).getByLabelText("平台代码"), "deepseek");
    await userEvent.click(within(panel).getByRole("button", { name: "保存平台" }));

    expect(
      await within(panel).findByText("服务地址与模型名都填上才能保存"),
    ).toBeInTheDocument();
  });

  it("编辑把已有平台填进表单", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "联网与模型平台" });
    const panel = heading.closest(".panel") as HTMLElement;

    await userEvent.click(within(panel).getAllByRole("button", { name: "编辑" })[0]!);
    expect(within(panel).getByLabelText("平台代码")).toHaveValue("local");
    expect(within(panel).getByLabelText("模型名")).toHaveValue("qwen2.5:14b");
  });

  it("测试连通展示探针结论与审计", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "联网与模型平台" });
    const panel = heading.closest(".panel") as HTMLElement;

    await userEvent.click(within(panel).getByRole("button", { name: "测试连通" }));
    expect(
      await within(panel).findByText(
        "探针连通正常，cloud / gpt-x，耗时 842 毫秒，审计 call-demo-probe",
      ),
    ).toBeInTheDocument();
  });

  it("体检清单按必备项给状态，并可一眼看到用量", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "体检清单" });
    const panel = heading.closest(".panel") as HTMLElement;

    // 默认没开联网，这一项应标成还没就绪。
    const networking = within(panel).getByText("联网能力").closest(".checkup") as HTMLElement;
    expect(networking).toHaveAttribute("data-ok", "false");
    expect(within(networking).getByText("已关闭")).toBeInTheDocument();

    // 大师包在演示数据里已装好，应算就绪。
    const masters = within(panel).getByText("大师包").closest(".checkup") as HTMLElement;
    expect(masters).toHaveAttribute("data-ok", "true");

    expect(within(panel).getByText("今日花费")).toBeInTheDocument();
    expect(within(panel).getByText("本月花费")).toBeInTheDocument();
  });

  it("历史回顾把各来源合并成一条流水", async () => {
    renderRealm();
    await showSystem();
    const heading = await screen.findByRole("heading", { name: "历史回顾" });
    const panel = heading.closest(".panel") as HTMLElement;

    const rows = panel.querySelectorAll(".call");
    expect(rows.length).toBeGreaterThan(0);
    // 只回顾最近的若干条，不把各来源的完整列表再铺一遍。
    expect(rows.length).toBeLessThanOrEqual(12);
    expect(within(panel).getAllByText("模型调用").length).toBeGreaterThan(0);
  });

  it("采集记录可生成录入，再次点击提示已生成过", async () => {
    const calls: string[] = [];
    const client = createCommandClient({
      invoke: async (name, request) => {
        calls.push(name);
        return stubTransport.invoke(name, request);
      },
    });
    render(
      <IpcProvider client={client}>
        <RealmHarness />
      </IpcProvider>,
    );

    await screen.findByText("采集台");
    await userEvent.click(screen.getAllByRole("button", { name: "生成录入" })[0]!);
    expect(await screen.findByText("已生成录入 · 去「炼」看")).toBeInTheDocument();
    expect(calls.filter((name) => name === "intake_create")).toHaveLength(1);

    await userEvent.click(screen.getAllByRole("button", { name: "生成录入" })[0]!);
    expect(await screen.findByText("这条已经生成过录入，不再重复")).toBeInTheDocument();
    expect(calls.filter((name) => name === "intake_create")).toHaveLength(1);
  });

  it("开启主动助学后可发起对撞并看到结果", async () => {
    renderRealm();
    const toggle = await screen.findByRole("switch", { name: "主动助学" });
    if (toggle.getAttribute("aria-checked") !== "true") {
      await userEvent.click(toggle);
    }
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "主动助学" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    await userEvent.type(
      screen.getByRole("textbox", { name: "对撞内容" }),
      "先扩张还是先收敛",
    );
    await userEvent.click(screen.getByRole("button", { name: "发起对撞" }));
    expect(
      await screen.findByText("两次「先扩张」的判断其实同源"),
    ).toBeInTheDocument();
    expect(screen.getByText(/对撞产生 2 条洞察/)).toBeInTheDocument();
  });
});

function dataPurgeButton() {
  return screen.getByRole("button", { name: /清除全部数据|确认清除，不可撤销/ });
}
