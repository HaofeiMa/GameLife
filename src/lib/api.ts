import { invoke } from "@tauri-apps/api/core";

import type { LiveWindow, QuestView } from "./questLive";

export type { LiveWindow, QuestView } from "./questLive";

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
  taskSnapshotJson?: string | null;
}

export interface TaskListView {
  id: string;
  name: string;
  sort: number;
  role: string;
}

export interface TaskView {
  id: string;
  listId: string;
  title: string;
  done: boolean;
  start: number | null;
  end: number | null;
  range: string | null;
}

export interface ParsedTaskView {
  title: string;
  listId: string;
  start: number | null;
  end: number | null;
  parseOk: boolean;
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
  quests: QuestView[];
  live: LiveWindow | null;
  previousWorkday: string | null;
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
  lists: TaskListView[];
  tasks: TaskView[];
  coinBalance: number;
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

export interface VisionProviderSettings {
  id: string;
  baseUrl: string;
  model: string;
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
  primaryProvider: string;
  fallbackProvider: string;
  visionProviders: VisionProviderSettings[];
}

export interface ProviderKeyStatus {
  opencodeGo: boolean;
  openai: boolean;
  custom: boolean;
}

export const BUILTIN_NEVER_CAPTURE = [
  "1Password",
  "Bitwarden",
  "Keychain Access",
];

export interface DayView {
  day: string;
  dayStart: number;
  tasks: TaskView[];
  slots: TodaySlot[];
}

export function getToday(): Promise<TodayView> {
  return invoke("get_today");
}

export function getDayView(day: string): Promise<DayView> {
  return invoke("get_day_view", { day });
}

export function getWeek(): Promise<WeekView> {
  return invoke("get_week");
}

export function setQuests(quests: QuestView[]): Promise<void> {
  return invoke("set_quests", { quests });
}

export function listTasks(): Promise<TodayView> {
  return invoke("list_tasks");
}

export function upsertTask(task: TaskView): Promise<void> {
  return invoke("upsert_task", { task });
}

export function toggleTaskDone(id: string, done: boolean): Promise<void> {
  return invoke("toggle_task_done", { id, done });
}

export function parseTaskLine(
  line: string,
  currentListId?: string | null,
): Promise<ParsedTaskView> {
  return invoke("parse_task_line", { line, currentListId });
}

export function createList(name: string): Promise<TaskListView> {
  return invoke("create_list", { name });
}

export function continuePreviousWorkday(): Promise<string> {
  return invoke("continue_previous_workday");
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

export function setProviderApiKey(provider: string, key: string): Promise<void> {
  return invoke("set_provider_api_key", { provider, key });
}

export function providerKeyStatus(): Promise<ProviderKeyStatus> {
  return invoke("provider_key_status");
}

export interface PermissionStatus {
  accessibility: boolean;
  screenRecording: boolean;
  processName: string;
  processPath: string;
}

export function getPermissionStatus(): Promise<PermissionStatus> {
  return invoke("get_permission_status");
}

export function requestScreenRecording(): Promise<boolean> {
  return invoke("request_screen_recording");
}
