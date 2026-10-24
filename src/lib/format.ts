/** 展示层格式化：UTC 存储 → 本地展示（D-010）。 */

export function fmtDurationMs(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '—';
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function fmtPercent(fraction: number | null | undefined, digits = 0): string {
  if (fraction === null || fraction === undefined) return '—';
  return `${(fraction * 100).toFixed(digits)}%`;
}

export function fmtUtcToLocal(rfc3339: string): string {
  const date = new Date(rfc3339);
  if (Number.isNaN(date.getTime())) return rfc3339;
  return date.toLocaleString('zh-CN', { hour12: false });
}
