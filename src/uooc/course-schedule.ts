export function courseSchedule(weeks: string) {
  const match = /^\s*(\d+)\s*\/\s*(\d+)\s*$/.exec(weeks);
  if (!match) return null;
  const current = Number(match[1]);
  const total = Number(match[2]);
  if (!Number.isSafeInteger(current) || !Number.isSafeInteger(total) || total <= 0) return null;

  const ratio = current / total;
  // One remaining week overrides the fractional thresholds, including short courses.
  const color =
    total - current <= 1
      ? "danger"
      : ratio >= 2 / 3
        ? "warning"
        : ratio >= 1 / 2
          ? "success"
          : "accent";
  return { percent: Math.min(100, ratio * 100), color } as const;
}
