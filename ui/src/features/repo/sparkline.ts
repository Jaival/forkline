/** SVG polyline `points` for a series, scaled into a `width` x `height` box. */
export function sparklinePoints(values: number[], width: number, height: number): string {
  if (values.length < 2) return "";
  const min = Math.min(...values);
  const span = Math.max(...values) - min || 1;
  return values
    .map((v, i) => {
      const x = (i / (values.length - 1)) * width;
      const y = height - ((v - min) / span) * height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

export type LanguageSegment = { name: string; percent: number };

/** Percent share per language, largest first. Empty when there are no bytes. */
export function languageShares(languages: Array<{ name: string; bytes: number }>): LanguageSegment[] {
  const total = languages.reduce((sum, l) => sum + l.bytes, 0);
  if (total === 0) return [];
  return languages
    .map((l) => ({ name: l.name, percent: (l.bytes / total) * 100 }))
    .sort((a, b) => b.percent - a.percent);
}

const COLORS = ["#dea584", "#3178c6", "#00add8", "#3572a5", "#89e051", "#e34c26", "#563d7c", "#b07219"];

/** Stable color per language name. */
export function languageColor(name: string): string {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return COLORS[h % COLORS.length]!;
}
