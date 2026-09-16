export const CLICK_SLOP_PX = 4;

export function withinClickSlop(dx: number, dy: number): boolean {
  return Math.hypot(dx, dy) <= CLICK_SLOP_PX;
}
