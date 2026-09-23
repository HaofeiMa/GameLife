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
  sort: number;
  repeat: string;
  remindOffsets: number[];
  notes: string;
}

export interface TaskBoardView {
  lists: TaskListView[];
  tasks: TaskView[];
}

export interface ParsedTaskView {
  title: string;
  listId: string;
  start: number | null;
  end: number | null;
  parseOk: boolean;
  spans: { start: number; end: number }[];
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
  /** Deferred settlement: coinsToday/xpToday are a preview, not ledger. */
  rewardsPending: boolean;
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
  /** Dominant category per local hour (0–23). Empty string = no slots. */
  hours: string[];
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

export interface RhythmBand {
  startHour: number;
  endHour: number;
  category: string;
}

export interface RhythmStartHour {
  day: string;
  hour: number | null;
  bands: RhythmBand[];
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
  /** "custom" | "codex". Empty/missing means a pre-migration preset. */
  kind?: string;
  /** Display name for a custom card. Empty falls back to 「自定义 API」. */
  name?: string;
  baseUrl: string;
  model: string;
}

export interface CategoryGuides {
  mainline: string;
  side: string;
  admin: string;
  entertainment: string;
}

export interface SyncSettings {
  enabled: boolean;
  /** "webdav" | "s3" */
  target: string;
  url: string;
  username: string;
  /** S3 only. */
  bucket: string;
  /** S3 only; "auto" for Cloudflare R2. */
  region: string;
  remotePath: string;
  intervalMinutes: number;
  /** "aggregate" | "samples" */
  scope: string;
  keepSnapshots: number;
  deviceLabel: string;
  /** Hours after a local day ends before stragglers stop blocking settlement. */
  settleGraceHours: number;
}

export interface SyncDevice {
  deviceId: string;
  label: string;
  platform: string;
  lastSeen: number;
}

export interface SyncStatus {
  enabled: boolean;
  target: string;
  url: string;
  scope: string;
  intervalMinutes: number;
  lastAt: number | null;
  lastOk: boolean;
  lastError: string;
  snapshotBytes: number;
  deviceId: string;
  devices: SyncDevice[];
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
  /** OS banners for scheduled task reminders. Default off. */
  taskNotifications: boolean;
  adminApps: string[];
  categoryGuides: CategoryGuides;
  /** system | light | dark — see src/lib/theme.ts */
  theme: string;
  /** default | qinglan | … — see src/lib/theme.ts COLOR_THEMES */
  colorTheme: string;
  /** Unused by the UI; kept so old config.json still loads. */
  todayTimelineMode: string;
  sync: SyncSettings;
  /** Pull unfinished TickTick tasks into the four preset board lists. Default off. */
  ticktickEnabled: boolean;
  ticktickClientId: string;
  /** Exact OAuth redirect registered in the TickTick developer console. */
  ticktickRedirectUri: string;
  /** projectId → ignore | mainline | side | longterm | chore */
  ticktickProjectRoles: Record<string, string>;
  /** columnId → ignore | mainline | side | longterm | chore. Missing key follows the list. */
  ticktickColumnRoles: Record<string, string>;
}

export interface ProviderKeyStatus {
  keys: Record<string, boolean>;
  codexLoggedIn: boolean;
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
  dayTasks: DayTask[];
}

export interface DayTask {
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

export function listTaskBoard(): Promise<TaskBoardView> {
  return invoke("list_task_board");
}

export interface TaskWriteResult {
  task: TaskView | null;
  warning: string | null;
}

export function upsertTask(task: TaskView): Promise<TaskWriteResult> {
  return invoke("upsert_task", { task });
}

export function toggleTaskDone(id: string, done: boolean): Promise<void> {
  return invoke("toggle_task_done", { id, done });
}

export function reorderTask(id: string, listId: string, sort: number): Promise<void> {
  return invoke("reorder_task", { id, listId, sort });
}

export function reorderList(id: string, sort: number): Promise<void> {
  return invoke("reorder_list", { id, sort });
}

export function duplicateTask(id: string): Promise<TaskWriteResult> {
  return invoke("duplicate_task", { id });
}

export function parseTaskLine(
  line: string,
  currentListId?: string | null,
): Promise<ParsedTaskView> {
  return invoke("parse_task_line", { line, currentListId });
}

export function createList(name: string, role: string): Promise<TaskListView> {
  return invoke("create_list", { name, role });
}

export function renameList(id: string, name: string): Promise<void> {
  return invoke("rename_list", { id, name });
}

export function deleteList(id: string): Promise<void> {
  return invoke("delete_list", { id });
}

export function deleteTask(id: string): Promise<void> {
  return invoke("delete_task", { id });
}

export function moveTask(id: string, listId: string): Promise<void> {
  return invoke("move_task", { id, listId });
}

export function rescheduleTask(
  id: string,
  start: number | null,
  end: number | null,
): Promise<void> {
  return invoke("reschedule_task", { id, start, end });
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

/**
 * Cached sync status. Reads `app_meta` only — opening 设置 must not hit the
 * network, so the remote is contacted by `syncNow` / `syncTestConnection`.
 */
export function syncStatus(): Promise<SyncStatus> {
  return invoke("sync_status");
}

export function syncNow(): Promise<SyncStatus> {
  return invoke("sync_now_cmd");
}

export function syncTestConnection(): Promise<string> {
  return invoke("sync_test_connection");
}

/** Write-only: the credential is never read back to the frontend. */
export function syncSetCredentials(password: string): Promise<void> {
  return invoke("sync_set_credentials", { password });
}

export function syncListDevices(): Promise<SyncDevice[]> {
  return invoke("sync_list_devices");
}

/** Stages the restored database next to the live one; returns its path. */
export function syncRestore(deviceId: string): Promise<string> {
  return invoke("sync_restore", { deviceId });
}

export interface TickTickProject {
  id: string;
  name: string;
  sortOrder: number;
  role: string;
}

export interface TickTickStatus {
  enabled: boolean;
  connected: boolean;
  clientId: string;
  projects: TickTickProject[];
  writeTargets: Record<string, string>;
  lastSyncAt: number | null;
  lastResult: string;
}

export function ticktickStatus(): Promise<TickTickStatus> {
  return invoke("ticktick_status");
}
export function ticktickSetClientSecret(secret: string): Promise<void> {
  return invoke("ticktick_set_client_secret", { secret });
}
export function ticktickConnect(): Promise<TickTickStatus> {
  return invoke("ticktick_connect");
}
/** Live command returns unit; reload with `ticktickStatus()` after success. */
export function ticktickDisconnect(): Promise<void> {
  return invoke("ticktick_disconnect");
}
export function ticktickRefreshProjects(): Promise<TickTickStatus> {
  return invoke("ticktick_refresh_projects");
}
export function ticktickSyncNow(): Promise<TickTickStatus> {
  return invoke("ticktick_sync_now");
}
