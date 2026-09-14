import { invoke } from "./invoke";

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
  durationMinutes: number | null;
  kind: string;
  spent: number;
  status: string;
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
  activity: SlotActivityMinutes;
  appTop: AppTopRow[];
  pendingCount: number;
}

export interface AppTopRow {
  name: string;
  minutes: number;
  dominant: string;
}

export interface WeekDayRow {
  day: string;
  core: number;
  side: number;
  chore: number;
}

export interface WeekHourRow {
  hour: number;
  core: number;
  observed: number;
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
  byDay: WeekDayRow[];
  byHour: WeekHourRow[];
  coreLabel: string;
  creditedTodayMinutes: number;
  coreHours: number;
  wowCoreDeltaMinutes: number | null;
  distractionObservedRatio: number;
  pendingOverResolved: number;
  daysGe6h: number;
  daysGe8h: number;
}

export interface MonthDayCell {
  day: string;
  creditedCore: number;
  isWeekend: boolean;
  isFuture: boolean;
}

export interface MonthReportView {
  days: MonthDayCell[];
  activity: SlotActivityMinutes;
  coinsEarned: number;
  coinsSpent: number;
  xpEarned: number;
  goldDays: number;
  freezeCount: number;
  completedDays: number;
}

export interface RhythmStartHour {
  day: string;
  hour: number | null;
}

export interface RhythmReportView {
  startHours: RhythmStartHour[];
  rate6h: number;
  rate8h: number;
  distractionRunCount: number;
  distractionRunSlots: number;
  peakHours: number[];
}

export interface AppReportRow {
  name: string;
  minutes: number;
  dominant: string;
  /** The list this app's time belongs to: what the rules decided, or the filing when no
   *  rule fired. */
  listedAs: string;
  /** True when the app's name itself sits in `listedAs`, rather than a title / URL rule
   *  having matched it. Only a filed app is re-filable from the table. */
  filed: boolean;
}

export interface HostReportRow {
  host: string;
  minutes: number;
  dominant: string;
}

export interface AppReportView {
  apps: AppReportRow[];
  newcomers: string[];
  hosts: HostReportRow[];
  protectedMinutes: number;
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

export interface CategoryGuides {
  mainline: string;
  side: string;
  admin: string;
  entertainment: string;
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
  showRailLabels: boolean;
  silentStart: boolean;
  adminApps: string[];
  categoryGuides: CategoryGuides;
  ticktickClientId: string;
  ticktickProjectRoles: Record<string, string>;
  ticktickColumnRoles: Record<string, string>;
  /** system | light | dark — see src/lib/theme.ts */
  theme: string;
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
  activity: SlotActivityMinutes;
  appTop: AppTopRow[];
  pendingCount: number;
  planMarks: PlanMark[];
  ticktickTasks: TickTickTask[];
}

export interface TickTickTask {
  id: string;
  title: string;
  role: string;
  start: number;
  end: number;
}

export interface PlanMark {
  start: number;
  end: number;
  title: string;
}

export function getToday(): Promise<TodayView> {
  return invoke("get_today");
}

export function getDayView(day: string): Promise<DayView> {
  return invoke("get_day_view", { day });
}

export type StatsRangeKind = "week" | "month";

/**
 * `anchor` is any day inside the wanted week. Omit it for the current week.
 * The wallet fields in the response (coin balance, XP today, live session)
 * are always "now" regardless of the anchor.
 */
export function getWeek(anchor?: string): Promise<WeekView> {
  return invoke("get_week", { anchor: anchor ?? null });
}

export function getMonthReport(year: number, month: number): Promise<MonthReportView> {
  return invoke("get_month_report", { year, month });
}

export function getRhythmReport(kind: StatsRangeKind, anchor: string): Promise<RhythmReportView> {
  return invoke("get_rhythm_report", { kind, anchor });
}

export function getAppReport(kind: StatsRangeKind, anchor: string): Promise<AppReportView> {
  return invoke("get_app_report", { kind, anchor });
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

export function saveSettings(
  settings: AppSettings,
  updatePolicy = true,
): Promise<void> {
  return invoke("save_settings", { settings, updatePolicy });
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

export interface ProviderTestResult {
  ok: boolean;
  preview: string;
}

export function testVisionProvider(provider?: string | null): Promise<ProviderTestResult> {
  return invoke("test_vision_provider", { provider });
}

export interface PermissionStatus {
  accessibility: boolean;
  screenRecording: boolean;
  processName: string;
  processPath: string;
}

export interface ObservationStatus {
  /** Whether this platform (and, on Linux, this session) can observe at all. */
  supported: boolean;
  /** `macOS` / `Windows` / `X11` / `Wayland` / `no X display`. */
  backend: string;
}

/** Platform capability, answered by the backend rather than sniffed from the UA. */
export function getObservationStatus(): Promise<ObservationStatus> {
  return invoke("observation_status");
}

export function getPermissionStatus(): Promise<PermissionStatus> {
  return invoke("get_permission_status");
}

export function requestScreenRecording(): Promise<boolean> {
  return invoke("request_screen_recording");
}

export function openPrivacySettings(kind: "accessibility" | "screen"): Promise<void> {
  return invoke("open_privacy_settings", { kind });
}

export interface TickTickStatus {
  connected: boolean;
  lastSync: number | null;
  lastError: string | null;
  secretPresent: boolean;
}

export interface TickTickAuthorize {
  authorizeUrl: string;
  listenOk: boolean;
  opened: boolean;
}

export function ticktickStatus(): Promise<TickTickStatus> {
  return invoke("ticktick_status");
}

export function ticktickSetClientSecret(secret: string): Promise<void> {
  return invoke("ticktick_set_client_secret", { secret });
}

export function ticktickBeginOauth(
  clientId?: string | null,
  clientSecret?: string | null,
): Promise<TickTickAuthorize> {
  return invoke("ticktick_begin_oauth", { clientId, clientSecret });
}

export function ticktickFinishOauth(
  callbackUrl: string,
  clientSecret?: string | null,
): Promise<void> {
  return invoke("ticktick_finish_oauth", { callbackUrl, clientSecret });
}

export function ticktickDisconnect(): Promise<void> {
  return invoke("ticktick_disconnect");
}

export interface TickTickSyncResult {
  count: number;
  truncated: boolean;
}

export function ticktickSync(): Promise<TickTickSyncResult> {
  return invoke("ticktick_sync");
}

export interface TickTickTreeColumn {
  id: string;
  name: string;
}

export interface TickTickTreeProject {
  id: string;
  name: string;
  columns: TickTickTreeColumn[];
}

export interface TickTickTree {
  fetchedAt: number;
  projects: TickTickTreeProject[];
}

/**
 * The cached project/column structure. Pass `refresh` to hit the API;
 * without it this reads the local cache and returns immediately, which is
 * what keeps opening 设置 → TickTick instant.
 */
export function ticktickTree(refresh = false): Promise<TickTickTree> {
  return invoke("ticktick_tree", { refresh });
}
