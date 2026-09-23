/** "5 minutes ago", "3 hours ago", "2 days ago" for epoch milliseconds, in the UI language. */
export function ago(ms: number, language: string, now = Date.now()): string {
  const minutes = Math.max(0, Math.round((now - ms) / 60_000));
  const rtf = new Intl.RelativeTimeFormat(language, { numeric: "auto" });
  if (minutes < 60) return rtf.format(-minutes, "minute");
  if (minutes < 60 * 48) return rtf.format(-Math.round(minutes / 60), "hour");
  return rtf.format(-Math.round(minutes / 1440), "day");
}
