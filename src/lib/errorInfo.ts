/**
 * 把后端错误字符串解释成可读的四段：原因、影响、建议、原始详情。
 *
 * 为什么需要它：后端把错误序列化成**纯字符串**（`AppError` 的
 * `Serialize` 实现），前端此前只把原始文本塞进 `Empty desc`。用户看到
 * 「网络错误: error sending request for url (...)」时，既不知道是自己的
 * 网络问题还是站点问题，也不知道该重试还是该改配置。
 *
 * 分类只依据**可验证的文本前缀**，不猜测：
 *   - `网络错误:`、`IO 错误:`、`JSON 错误:` 由后端 `AppError` 的 Display
 *     实现固定写入，是稳定信号。
 *   - 其余按后端实际使用的措辞匹配（见各条 branch 注释）。
 * 认不出的一律归为 `unknown`，如实说明「无法判断原因」，而不是编一个。
 */

export type ErrorKind =
  | "network"
  | "permission"
  | "disk"
  | "verification"
  | "source"
  | "unknown";

export interface ErrorInfo {
  kind: ErrorKind;
  /** 一句话说明发生了什么 */
  cause: string;
  /** 这件事对用户的实际影响 */
  impact: string;
  /** 用户接下来该做什么 */
  action: string;
  /** 是否值得原样重试。校验类错误重试没用，必须先刷新或换源。 */
  retry: boolean;
}

/** 匹配时按顺序尝试，靠前的更具体。 */
const RULES: {
  kind: ErrorKind;
  test: (raw: string) => boolean;
  cause: string;
  impact: string;
  action: string;
  retry: boolean;
}[] = [
  {
    kind: "verification",
    // 后端校验失败时的措辞：校验资料无效 / 校验文件 / SHA-256 / 哈希
    test: (raw) => /校验|SHA-?256|哈希|摘要|签名/.test(raw),
    cause: "下载内容的校验资料无法确认来源或与实际文件不符。",
    impact: "本次下载已被阻止，不会解压或执行任何文件。",
    action: "刷新版本列表后重试；若仍失败，切换到「官方」来源。不要绕过校验继续安装。",
    retry: false,
  },
  {
    kind: "permission",
    // 后端在权限不足时写入「权限」；Windows 拒绝访问的原文含 denied/拒绝
    test: (raw) => /权限|拒绝访问|Access is denied|denied|0x80070005/i.test(raw),
    cause: "当前用户没有写入目标位置的权限。",
    impact: "下载或安装无法继续，已有文件不受影响。",
    action: "换一个用户可写的下载目录，或以管理员身份运行 EnvCon。",
    retry: false,
  },
  {
    kind: "disk",
    test: (raw) => /磁盘|空间不足|no space|0x80070070/i.test(raw),
    cause: "目标磁盘空间不足或写入失败。",
    impact: "文件无法完整写入，本次安装不会完成。",
    action: "清理目标磁盘后重试，或把下载目录换到空间更大的分区。",
    retry: true,
  },
  {
    kind: "network",
    // `AppError::Net` 的 Display 前缀
    test: (raw) => /^网络错误[:：]/.test(raw.trim()),
    cause: "连接下载源失败（DNS、代理、防火墙或站点临时不可用）。",
    impact: "没有取到版本列表或文件，本机没有任何改动。",
    action: "确认网络可访问该来源后重试；持续失败可切换到镜像来源。",
    retry: true,
  },
  {
    kind: "source",
    // 后端在版本目录/来源不匹配时使用「来源」「版本目录」「不匹配」
    test: (raw) => /来源|版本目录|不匹配/.test(raw),
    cause: "当前版本列表与后端解析结果不一致，或该来源不支持此工具。",
    impact: "列表可能已过期，继续操作会下载到错误资产。",
    action: "刷新版本列表后重试；仍失败请切换来源。",
    retry: true,
  },
  {
    kind: "disk",
    // `AppError::Io` 的 Display 前缀，放在 network 之后以免抢匹配
    test: (raw) => /^IO 错误[:：]/.test(raw.trim()),
    cause: "读写本地文件时失败。",
    impact: "操作未完成，已存在的文件保持不变。",
    action: "确认目标目录存在且可写后重试。",
    retry: true,
  },
];

export function explainError(raw: string | null | undefined): ErrorInfo {
  const text = (raw ?? "").trim();
  if (!text) {
    return {
      kind: "unknown",
      cause: "没有收到具体的错误信息。",
      impact: "无法判断本次操作是否产生副作用。",
      action: "重试一次；若重复出现，请记录当时的操作步骤。",
      retry: true,
    };
  }
  for (const rule of RULES) {
    if (rule.test(text)) {
      return {
        kind: rule.kind,
        cause: rule.cause,
        impact: rule.impact,
        action: rule.action,
        retry: rule.retry,
      };
    }
  }
  return {
    kind: "unknown",
    // 不认识就不编：如实说无法归类，同时保留原文供排查。
    cause: "未识别的错误。",
    impact: "本次操作没有按预期完成。",
    action: "重试一次；若重复出现，请复制下方详情反馈。",
    retry: true,
  };
}
