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
