import { invoke } from "@tauri-apps/api/core";

export interface ChestGold {
  unlocked: boolean;
  have: number;
  need: number;
}

export interface SlotActivityMinutes {
  core: number;
  support: number;
  admin: number;
  side: number;
  distraction: number;
  away: number;
  unobserved: number;
}

export interface TodaySlot {
  start: number;
  dominant: string;
  creditedMinutes: number;
  activity: SlotActivityMinutes;
  activitySummary: string;
  pending: boolean;
  final: boolean;
}

export interface EntertainmentView {
  name: string;
  endsAt: number;
  remainingSecs: number;
}

export interface EndedEntertainmentView {
  name: string;
}

export interface LedgerTailRow {
  key: string;
  coin: number;
  xp: number;
  ts: number;
}

export interface RedemptionView {
  id: string;
  name: string;
  ts: number;
}

export interface TodayView {
  day: string;
  quests: string[];
  creditedSeconds: number;
  creditedLabel: string;
  coinsToday: number;
  xpToday: number;
  xpShopUnlocked: boolean;
  chest: ChestGold;
  gold: ChestGold;
  streak: number;
  atRisk: boolean;
  freezeCandidates: string[];
  defaultFreezeDate: string | null;
  firstCoreLabel: string | null;
  slots: TodaySlot[];
  goldDay: boolean;
  activeEntertainment: EntertainmentView | null;
  endedEntertainment: EndedEntertainmentView | null;
  ledgerTail: LedgerTailRow[];
}

export interface WeekView {
  core: number;
  support: number;
  admin: number;
  side: number;
  distraction: number;
  away: number;
  unobserved: number;
  pendingReview: number;
  wishes: WishView[];
  coinBalance: number;
  xpToday: number;
  xpShopUnlocked: boolean;
  activeEntertainment: EntertainmentView | null;
  endedEntertainment: EndedEntertainmentView | null;
  redemptions: RedemptionView[];
}

export interface WishView {
  id: string;
  name: string;
  kind: string;
  price: number;
  durationMinutes: number | null;
}

export interface AppSettings {
  screenshotRetention: string;
  sampleKeepDays: number;
  loginAtStartup: boolean;
  trustedApps: string[];
  distractionRules: string[];
  sideProjectRules: string[];
  readingApps: string[];
  neverCaptureApps: string[];
}

export const BUILTIN_NEVER_CAPTURE = [
  "1Password",
  "Bitwarden",
  "Keychain Access",
];

export function getToday(): Promise<TodayView> {
  return invoke("get_today");
}

export function getWeek(): Promise<WeekView> {
  return invoke("get_week");
}

export function setQuests(quests: string[]): Promise<void> {
  return invoke("set_quests", { quests });
}

export function reviewSlot(
  day: string,
  slotStart: number,
  category: string,
): Promise<void> {
  return invoke("review_slot", { day, slotStart, category });
}

export function reportMisclassification(
  day: string,
  slotStart: number,
  note: string,
): Promise<void> {
  return invoke("report_misclassification", { day, slotStart, note });
}

export function redeem(wishId: string, redemptionId: string): Promise<void> {
  return invoke("redeem", { wishId, redemptionId });
}

export function createWish(
  id: string,
  name: string,
  kind: string,
  price: number,
  durationMinutes: number | null,
): Promise<void> {
  return invoke("create_wish", {
    id,
    name,
    kind,
    price,
    durationMinutes,
  });
}

export function updateWish(
  id: string,
  name: string,
  price: number,
  durationMinutes: number | null,
): Promise<void> {
  return invoke("update_wish", { id, name, price, durationMinutes });
}

export function archiveWish(id: string): Promise<void> {
  return invoke("archive_wish", { id });
}

export function endToday(): Promise<void> {
  return invoke("end_today");
}

export function freezeDay(protectedDate: string): Promise<void> {
  return invoke("freeze", { protectedDate });
}

export function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export function saveSettings(settings: AppSettings): Promise<void> {
  return invoke("save_settings", { settings });
}

export function setApiKey(key: string): Promise<void> {
  return invoke("set_api_key", { key });
}

export function hasApiKey(): Promise<boolean> {
  return invoke("has_api_key");
}

export interface PermissionStatus {
  accessibility: boolean;
  screenRecording: boolean;
}

export function getPermissionStatus(): Promise<PermissionStatus> {
  return invoke("get_permission_status");
}
