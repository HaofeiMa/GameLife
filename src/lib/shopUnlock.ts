export function minutesUntilShopUnlock(creditedTodayMinutes: number): number {
  return Math.max(0, 60 - creditedTodayMinutes);
}
