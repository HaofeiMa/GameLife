/** Mirrors Rust `format_estimated_minutes`: floor to whole minutes, never show seconds. */
export function formatEstimatedMinutes(secs: number): string {
  const totalMinutes = Math.max(0, Math.floor(secs / 60));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return `${hours}h ${minutes}m`;
}
