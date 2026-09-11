export function heatTone(core: number, observed: number): number {
  if (observed <= 0) return 0;
  return Math.min(1, Math.max(0, core / observed));
}
