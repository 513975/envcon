export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "—";
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** i).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return "—";
  return `${formatBytes(bytesPerSec)}/s`;
}

/**
 * 剩余时间估算。只在**能真实计算**时返回结果，否则返回 null。
 *
 * 为什么不做平滑：调用方每秒拿到的是瞬时速度，用它算出的 ETA 会来回跳。
 * 与其在这里假装精确，不如让调用方决定是否显示——本项目的一条既有原则是
 * 不显示虚假百分比，剩余时间同理。
 *
 * 返回 null 的情形（都不应显示数字）：
 *   - 总大小未知：分母不存在，任何数字都是编的
 *   - 速度非正：还没开始传输，或连接刚断
 *   - 已经下完：没有"剩余"
 *   - 估算超过 24 小时：说明速度极低或刚开始，给一个大数只会误导
 */
export function formatEta(
  remainingBytes: number,
  bytesPerSec: number,
): string | null {
  if (!Number.isFinite(remainingBytes) || remainingBytes <= 0) return null;
  if (!Number.isFinite(bytesPerSec) || bytesPerSec <= 0) return null;
  const seconds = remainingBytes / bytesPerSec;
  if (!Number.isFinite(seconds) || seconds > 86400) return null;
  if (seconds < 1) return "即将完成";
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `剩余约 ${h} 小时 ${m} 分`;
  if (m > 0) return `剩余约 ${m} 分 ${s} 秒`;
  return `剩余约 ${s} 秒`;
}

export function formatDate(date: string | null | undefined): string {
  if (!date) return "—";
  // A bare year plus time was previously misparsed by JS as January 1 of that year.
  if (/^\d{4}\s+\d{1,2}:\d{2}$/.test(date)) return "日期未知";
  const calendarDate = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  if (calendarDate) {
    const [, year, month, day] = calendarDate;
    const d = new Date(Number(year), Number(month) - 1, Number(day));
    if (
      d.getFullYear() === Number(year) &&
      d.getMonth() === Number(month) - 1 &&
      d.getDate() === Number(day)
    ) {
      return d.toLocaleDateString("zh-CN", {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
      });
    }
  }
  const d = new Date(date);
  if (isNaN(d.getTime())) return date;
  return d.toLocaleDateString("zh-CN", { year: "numeric", month: "2-digit", day: "2-digit" });
}

export function cn(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(" ");
}
