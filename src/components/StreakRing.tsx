import { categoryColor, categoryColorAt } from "../lib/theme";

export interface StreakRingProps {
  streak: number;
  atRisk: boolean;
  size?: number;
}

/** Streak as a ring. The arc fills over a 30-day cycle rather than
 *  tracking any real quota — it reads as "how deep into the run you are". */
export function StreakRing({ streak, atRisk, size = 56 }: StreakRingProps) {
  const r = 15;
  const circumference = 2 * Math.PI * r;
  const fraction = streak <= 0 ? 0 : Math.min(1, 0.12 + (streak % 30) / 30);
  const stroke = atRisk ? "hsl(var(--warning))" : categoryColor("mainline");

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 40 40"
      className="shrink-0 text-foreground"
      role="img"
      aria-label={`连胜 ${streak} 天`}
    >
      <circle
        cx="20"
        cy="20"
        r={r}
        fill="none"
        stroke={categoryColorAt("away", 100)}
        strokeWidth="4"
      />
      <circle
        cx="20"
        cy="20"
        r={r}
        fill="none"
        stroke={stroke}
        strokeWidth="4"
        strokeDasharray={`${circumference * fraction} ${circumference}`}
        strokeLinecap={fraction > 0 ? "round" : "butt"}
        transform="rotate(-90 20 20)"
      />
      <text
        x="20"
        y="20"
        textAnchor="middle"
        dominantBaseline="central"
        fontSize="13"
        fontWeight="700"
        fill="currentColor"
      >
        {streak}
      </text>
    </svg>
  );
}
