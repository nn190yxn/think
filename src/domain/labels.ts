/**
 * 面向用户的统一说法。
 *
 * 界面只说人话：机器侧的枚举、错误码、字段名与计量单位都在这里翻译一次，
 * 避免同一件事在不同境界被叫成两个名字，也避免裸标识符直接出现在界面上。
 */

import type { CommandErrorCode } from "../ipc/protocol";

/**
 * 错误码的可读说法。原始码仍保留在错误对象里，只用于排查，
 * 不直接展示给用户。
 */
const ERROR_TEXT: Record<CommandErrorCode, string> = {
  E_INVALID_INPUT: "填写的内容不符合要求",
  E_NOT_FOUND: "没有找到要操作的内容",
  E_DB: "读写本机数据时出错",
  E_MIGRATION: "数据升级时出错",
  E_IO: "读写文件时出错",
  E_PACK_INVALID: "这个大师包不完整或格式不对",
  E_NETWORK_OFF: "联网能力没有开启",
  E_MODEL_UNAVAILABLE: "模型暂时不可用",
  E_MALFORMED_RESPONSE: "收到的回复格式不对",
  E_UNKNOWN: "操作没能完成",
};

/** 把机器错误变成一句人话；`message` 是后端给的具体原因。 */
export function readableError(code: string, message?: string): string {
  const base = ERROR_TEXT[code as CommandErrorCode] ?? "操作没能完成";
  const reason = message?.trim();
  if (!reason || reason === base) {
    return base;
  }
  return `${base}：${reason}`;
}

/** 大师在会诊里的发言角色。 */
const ROLE_LABEL: Record<string, string> = {
  answer: "独立作答",
  cross: "互相追问",
  synthesis: "汇总结论",
  fail: "未能作答",
  failed: "未能作答",
};

export function roleLabel(role: string): string {
  return ROLE_LABEL[role] ?? role;
}

/** 一次对外调用的结果。 */
const CALL_STATUS_LABEL: Record<string, string> = {
  ok: "成功",
  failed: "失败",
  error: "失败",
  timeout: "超时",
  denied: "未授权",
  skipped: "已跳过",
};

export function callStatusLabel(status: string): string {
  return CALL_STATUS_LABEL[status] ?? status;
}

/** 模型平台配置状态。 */
const PLATFORM_STATUS_LABEL: Record<string, string> = {
  ready: "已接入",
  disabled: "已配置，未启用",
  unconfigured: "还没配置",
};

export function platformStatusLabel(status: string): string {
  return PLATFORM_STATUS_LABEL[status] ?? status;
}

/** 外部数据源的配置状态。 */
const CONNECTOR_STATUS_LABEL: Record<string, string> = {
  ok: "可用",
  ready: "可用",
  unconfigured: "还没配置",
  failed: "连接失败",
  error: "连接失败",
  disabled: "已停用",
};

export function connectorStatusLabel(status: string): string {
  return CONNECTOR_STATUS_LABEL[status] ?? status;
}

/** 连接的种类。 */
const SOURCE_KIND_LABEL: Record<string, string> = {
  search: "搜索",
  page: "网页阅读",
  mcp: "外部工具",
};

export function sourceKindLabel(kind: string): string {
  return SOURCE_KIND_LABEL[kind] ?? kind;
}

/** 一个念头是怎么进到网络里的。 */
const NODE_SOURCE_LABEL: Record<string, string> = {
  thought_record: "思考记录",
  council_turn: "会诊发言",
  council_session: "会诊结论",
  principle_promotion: "沉淀的原则",
  companion: "主动助学",
  consolidation: "记忆固化",
};

export function nodeSourceLabel(kind: string): string {
  return NODE_SOURCE_LABEL[kind] ?? kind;
}

/** 对外发送问句的方式。 */
const QUERY_MODE_LABEL: Record<string, string> = {
  keyword: "关键词模式，只发送抽取出的关键词",
  question: "问句模式，隐去敏感信息后整句发送",
};

export function queryModeLabel(mode: string): string {
  return QUERY_MODE_LABEL[mode] ?? mode;
}

/** 蒸馏（提炼）任务的阶段。 */
const DISTILL_STAGE_LABEL: Record<string, string> = {
  distill_extract: "提取要点",
  distill_compose: "整理成技能",
  distill_verify: "校验技能",
};

export function distillStageLabel(stage: string): string {
  return DISTILL_STAGE_LABEL[stage] ?? stage;
}

/** 蒸馏（提炼）任务的状态。 */
const DISTILL_STATE_LABEL: Record<string, string> = {
  queued: "排队中",
  running: "进行中",
  done: "已完成",
  failed: "没成功",
  cancelled: "已取消",
};

export function distillStateLabel(state: string): string {
  return DISTILL_STATE_LABEL[state] ?? state;
}

/** 入库任务的两条通道与状态。 */
const INTAKE_MODE_LABEL: Record<string, string> = {
  manual: "手动添加",
  discovery: "主动搜集",
};

const INTAKE_STATE_LABEL: Record<string, string> = {
  pending: "待确认",
  adopted: "已采纳",
  rejected: "已剔除",
  queued: "排队中",
  done: "已完成",
  failed: "没成功",
};

export function intakeModeLabel(mode: string): string {
  return INTAKE_MODE_LABEL[mode] ?? mode;
}

export function intakeStateLabel(state: string): string {
  return INTAKE_STATE_LABEL[state] ?? state;
}

/** 分歧的判定方式。 */
const DIVERGENCE_MODE_LABEL: Record<string, string> = {
  lexical: "按用词重合度判断",
  polarity: "按观点方向判断",
  hybrid: "按用词与观点方向共同判断",
};

export function divergenceModeLabel(mode: string): string {
  return DIVERGENCE_MODE_LABEL[mode] ?? mode;
}

/** 主动搜集命中的来源类型。 */
const DISCOVERY_SOURCE_LABEL: Record<string, string> = {
  rss: "订阅源",
  web: "网页",
  corpus: "本机资料",
  file: "本地文件",
};

export function discoverySourceLabel(kind: string): string {
  return DISCOVERY_SOURCE_LABEL[kind] ?? kind;
}

/** 时长：界面一律说毫秒与秒，不再出现裸 `ms`。 */
export function formatDuration(milliseconds: number): string {
  if (milliseconds < 1000) {
    return `${milliseconds} 毫秒`;
  }
  return `${(milliseconds / 1000).toFixed(1)} 秒`;
}

/** 文件体积。 */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} 字节`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} 千字节`;
  }
  return `${(bytes / 1024 / 1024).toFixed(1)} 兆字节`;
}

/**
 * 金额。内部按百万分之一元记账，这里换算成元；
 * 不足一分钱的零头保留四位小数，避免小额被显示成 0。
 */
export function formatMoney(micros: number, currency: string): string {
  const unit = currency === "CNY" ? "元" : currency;
  return `${(micros / 1_000_000).toFixed(4)} ${unit}`;
}

/** 时间：列表里只到分钟，最近的记录补上日期。 */
export function formatTime(stamp: string, options?: { readonly dateOnly?: boolean }): string {
  const text = stamp.trim();
  if (!text) {
    return "";
  }
  if (options?.dateOnly) {
    return text.slice(0, 10);
  }
  return text.slice(0, 16).replace("T", " ");
}

/** 采集事件里能直接读懂的那一段，找不到时给一个中性说法。 */
export function captureExcerpt(payload: unknown): string {
  if (typeof payload === "object" && payload !== null) {
    const record = payload as Record<string, unknown>;
    for (const key of ["text", "title", "path", "imageRef"]) {
      const value = record[key];
      if (typeof value === "string" && value.trim()) {
        return value.slice(0, 80);
      }
    }
  }
  return "";
}

/** 采集能力的名字。 */
const CAPTURE_KIND_LABEL: Record<string, string> = {
  clipboard_text: "剪贴板文本",
  clipboard_image: "剪贴板图片",
  window: "当前窗口",
  file: "文件活动",
};

export function captureKindLabel(kind: string): string {
  return CAPTURE_KIND_LABEL[kind] ?? kind;
}

/** 一次记忆固化的触发方式。 */
const CONSOLIDATION_MODE_LABEL: Record<string, string> = {
  manual: "手动触发",
  idle: "空闲时自动",
};

export function consolidationModeLabel(mode: string): string {
  return CONSOLIDATION_MODE_LABEL[mode] ?? mode;
}

/**
 * 一次对外调用是为了做什么。模型调用与外部数据源调用共用这张表，
 * 保证同一件事在「模型调用记录」和「外部数据源调用记录」里说法一致。
 */
const CALL_PURPOSE_LABEL: Record<string, string> = {
  council_background: "会诊前搜集共享背景",
  council_seat_search: "参与者自行补充资料",
  council_round1: "第一轮独立作答",
  council_cross: "参与者互相追问",
  council_synthesis: "汇总结论",
  council_followup: "追问补充",
  council_polarity: "判断观点分歧",
  connector_test: "测试是否能连上",
  model_probe: "检查模型是否可用",
  self_distill: "提炼你的自我画像",
  distill_skeleton: "起草技能大纲",
  distill_extract: "提取要点",
  distill_compose: "整理成技能",
  distill_verify: "校验技能",
  distill_stress: "压力测试",
  distill_discovery: "主动搜集",
};

export function callPurposeLabel(purpose: string): string {
  return CALL_PURPOSE_LABEL[purpose] ?? purpose;
}

/** 追问所针对的位置。 */
const ANCHOR_KIND_LABEL: Record<string, string> = {
  conclusion: "整场结论",
  answer: "某位大师的作答",
  critique: "某条质疑",
  divergence: "某处分歧",
};

export function anchorKindLabel(kind: string): string {
  return ANCHOR_KIND_LABEL[kind] ?? kind;
}

/** 大师档案是否可用。 */
const MASTER_STATUS_LABEL: Record<string, string> = {
  ready: "可用",
  archived: "已归档",
  disabled: "已停用",
  incomplete: "资料不全",
};

export function masterStatusLabel(status: string): string {
  return MASTER_STATUS_LABEL[status] ?? status;
}
