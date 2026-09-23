/**
 * Demo data for README screenshots. Names, paths and totals are invented;
 * nothing here is loaded from the developer's database or secrets.json.
 */
import { formatEstimatedMinutes } from "../format";
import type {
  AppReportView,
  AppSettings,
  AppTopRow,
  DayView,
  MonthReportView,
  ObservationStatus,
  PermissionStatus,
  ProviderKeyStatus,
  RhythmBand,
  RhythmReportView,
  RhythmStartHour,
  SlotActivityMinutes,
  SyncStatus,
  TickTickStatus,
  TodaySlot,
  TodayView,
  TaskBoardView,
  WeekView,
  WishView,
} from "../api";

const emptyActivity = (): SlotActivityMinutes => ({
  core: 0,
  support: 0,
  admin: 0,
  side: 0,
  distraction: 0,
  away: 0,
  unobserved: 0,
});

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

function isoFrom(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function atLocalMidnight(iso: string): Date {
  return new Date(`${iso}T00:00:00`);
}

function addDaysIso(iso: string, days: number): string {
  const d = atLocalMidnight(iso);
  d.setDate(d.getDate() + days);
  return isoFrom(d);
}

function mondayOf(iso: string): string {
  const d = atLocalMidnight(iso);
  const weekday = d.getDay();
  const back = weekday === 0 ? 6 : weekday - 1;
  d.setDate(d.getDate() - back);
  return isoFrom(d);
}

function daysInMonth(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

const TODAY = isoFrom(new Date());
const DAY_START = Math.floor(atLocalMidnight(TODAY).getTime() / 1000);

export const PREVIEW_TASK_BOARD: TaskBoardView = {
  lists: [
    { id: "list-mainline", name: "主线任务", sort: 0, role: "mainline" },
    { id: "list-side", name: "支线任务", sort: 1, role: "side" },
    { id: "list-chore", name: "杂项", sort: 2, role: "chore" },
    { id: "list-longterm", name: "长期规划", sort: 3, role: "longterm" },
  ],
  tasks: [
    {
      id: "task-preview-1",
      listId: "list-mainline",
      title: "写方法节",
      done: false,
      start: DAY_START + 10 * 3600,
      end: DAY_START + 12 * 3600,
      range: null,
      sort: 0,
      repeat: "none",
      remindOffsets: [],
      notes: "",
    },
    {
      id: "task-preview-yest",
      listId: "list-mainline",
      title: "整理昨日数据",
      done: false,
      start: DAY_START - 86400 + 16 * 3600,
      end: DAY_START - 86400 + 17 * 3600,
      range: null,
      sort: 1,
      repeat: "none",
      remindOffsets: [],
      notes: "",
    },
    {
      id: "task-preview-tom",
      listId: "list-mainline",
      title: "组会",
      done: false,
      start: DAY_START + 86400 + 10 * 3600,
      end: DAY_START + 86400 + 11 * 3600,
      range: null,
      sort: 2,
      repeat: "weekly",
      remindOffsets: [],
      notes: "",
    },
    {
      id: "task-preview-next",
      listId: "list-mainline",
      title: "下周二讨论",
      done: false,
      start: DAY_START + 6 * 86400 + 14 * 3600,
      end: DAY_START + 6 * 86400 + 15 * 3600,
      range: null,
      sort: 3,
      repeat: "none",
      remindOffsets: [],
      notes: "",
    },
    {
      id: "task-preview-far",
      listId: "list-mainline",
      title: "九月底截稿",
      done: false,
      start: DAY_START + 14 * 86400 + 9 * 3600,
      end: DAY_START + 14 * 86400 + 10 * 3600,
      range: null,
      sort: 4,
      repeat: "none",
      remindOffsets: [],
      notes: "",
    },
    {
      id: "task-preview-2",
      listId: "list-side",
      title: "整理引用",
      done: false,
      start: null,
      end: null,
      range: null,
      sort: 1,
      repeat: "none",
      remindOffsets: [],
      notes: "地点：A301",
    },
  ],
};

type Role = TodaySlot["dominant"];

function slotAt(
  hour: number,
  minute: number,
  dominant: Role,
  extra?: Partial<TodaySlot>,
): TodaySlot {
  const start = DAY_START + hour * 3600 + minute * 60;
  const activity = emptyActivity();
  if (dominant === "core_research" || dominant === "core") activity.core = 15;
  else if (dominant === "research_support" || dominant === "support") {
    activity.support = 15;
  } else if (dominant === "admin") activity.admin = 15;
  else if (dominant === "side_project" || dominant === "side") activity.side = 15;
  else if (dominant === "distraction") activity.distraction = 15;
  else if (dominant === "break_away" || dominant === "away") activity.away = 15;
  else activity.unobserved = 15;
  const credited =
    extra?.creditedMinutes ??
    (dominant === "core_research" || dominant === "core" ? 15 : 0);
  return {
    start,
    dominant,
    creditedMinutes: credited,
    activity,
    activitySummary:
      dominant === "core_research" || dominant === "core"
        ? "Cursor"
        : dominant === "admin"
          ? "邮件"
          : dominant === "side_project" || dominant === "side"
            ? "Preview"
            : dominant === "distraction"
              ? "视频"
              : "—",
    pending: false,
    final: true,
    ...extra,
  };
}

/** A made-up weekday: deep work in the morning, a break, then a mixed afternoon. */
function buildSlots(): TodaySlot[] {
  const slots: TodaySlot[] = [];
  const paint = (
    fromH: number,
    fromM: number,
    toH: number,
    toM: number,
    dominant: Role,
    extra?: Partial<TodaySlot>,
  ) => {
    let t = fromH * 60 + fromM;
    const end = toH * 60 + toM;
    while (t < end) {
      slots.push(slotAt(Math.floor(t / 60), t % 60, dominant, extra));
      t += 15;
    }
  };
  paint(8, 0, 8, 30, "unobserved");
  paint(8, 30, 12, 0, "core_research");
  paint(12, 0, 13, 0, "break_away");
  paint(13, 0, 15, 0, "core_research");
  paint(15, 0, 15, 30, "admin");
  paint(15, 30, 16, 0, "distraction");
  paint(16, 0, 18, 0, "core_research");
  paint(18, 0, 18, 30, "side_project");
  return slots;
}

const SLOTS = buildSlots();

const ACTIVITY: SlotActivityMinutes = SLOTS.reduce((acc, slot) => {
  acc.core += slot.activity.core;
  acc.support += slot.activity.support;
  acc.admin += slot.activity.admin;
  acc.side += slot.activity.side;
  acc.distraction += slot.activity.distraction;
  acc.away += slot.activity.away;
  acc.unobserved += slot.activity.unobserved;
  return acc;
}, emptyActivity());

const CREDITED_SECONDS = SLOTS.reduce((n, slot) => n + slot.creditedMinutes * 60, 0);

const APP_TOP: AppTopRow[] = [
  { name: "Cursor", minutes: 248, dominant: "core" },
  { name: "Preview", minutes: 32, dominant: "side" },
  { name: "Mail", minutes: 28, dominant: "admin" },
  { name: "YouTube", minutes: 24, dominant: "distraction" },
];

const WISHES: WishView[] = [
  { id: "coffee", name: "手冲咖啡", kind: "coin", price: 48, durationMinutes: null },
  { id: "film", name: "胶卷一筒", kind: "coin", price: 96, durationMinutes: null },
  { id: "keyboard", name: "机械键盘", kind: "coin", price: 640, durationMinutes: null },
  { id: "walk", name: "出门散步", kind: "xp", price: 40, durationMinutes: 20 },
  { id: "game", name: "打一局游戏", kind: "xp", price: 90, durationMinutes: 40 },
];

const NOW_SECS = Math.floor(Date.now() / 1000);
const WALK_REMAIN_SECS = 12 * 60 + 40;
const WALK_ENDS_AT = NOW_SECS + WALK_REMAIN_SECS;

export const PREVIEW_TODAY: TodayView = {
  day: TODAY,
  quests: [],
  live: {
    app: "Cursor",
    title: "App.tsx — demo-app",
    documentPath: "/Users/demo/demo-app/src/App.tsx",
    url: null,
    matchedQuestIndex: null,
    trusted: true,
  },
  previousWorkday: addDaysIso(TODAY, -3),
  creditedSeconds: CREDITED_SECONDS,
  creditedLabel: formatEstimatedMinutes(CREDITED_SECONDS),
  coinsToday: 18,
  xpToday: 200,
  xpShopUnlocked: true,
  chest: { unlocked: false, have: CREDITED_SECONDS, need: 21600 },
  gold: { unlocked: false, have: CREDITED_SECONDS, need: 28800 },
  streak: 12,
  atRisk: false,
  freezeCandidates: [addDaysIso(TODAY, -1)],
  defaultFreezeDate: addDaysIso(TODAY, -1),
  firstCoreLabel: "08:30",
  slots: SLOTS,
  goldDay: false,
  activeEntertainment: null,
  endedEntertainment: null,
  ledgerTail: [
    { key: "demo-tick-1", coin: 1, xp: 10, ts: DAY_START + 10 * 3600 },
    { key: "demo-tick-2", coin: 1, xp: 10, ts: DAY_START + 11 * 3600 },
  ],
  lists: [],
  tasks: [],
  coinBalance: 1280,
  activity: ACTIVITY,
  appTop: APP_TOP,
  pendingCount: 0,
  rewardsPending: false,
};

export function previewDayView(day: string): DayView {
  const start = Math.floor(atLocalMidnight(day).getTime() / 1000);
  const sameDay = day === TODAY;
  const slots = sameDay
    ? SLOTS
    : [
        {
          ...slotAt(9, 0, "core"),
          start: start + 9 * 3600,
        },
        {
          ...slotAt(14, 0, "side"),
          start: start + 14 * 3600,
          creditedMinutes: 0,
        },
      ];
  return {
    day,
    dayStart: start,
    tasks: [],
    slots,
    activity: sameDay ? ACTIVITY : { ...emptyActivity(), core: 120, side: 45 },
    appTop: sameDay ? APP_TOP : [{ name: "Cursor", minutes: 90, dominant: "core" }],
    pendingCount: 0,
    planMarks: sameDay
      ? [
          {
            start: DAY_START + 9 * 3600,
            end: DAY_START + 11 * 3600,
            title: "写设计说明",
          },
          {
            start: DAY_START + 14 * 3600,
            end: DAY_START + 15 * 3600,
            title: "代码评审",
          },
        ]
      : [],
    dayTasks: sameDay
      ? [
          {
            id: "tt-1",
            title: "写设计说明：判断管线与计划槽对照",
            role: "mainline",
            start: DAY_START + 9 * 3600,
            end: DAY_START + 11 * 3600,
          },
          {
            id: "tt-2",
            title: "代码评审",
            role: "mainline",
            start: DAY_START + 14 * 3600,
            end: DAY_START + 15 * 3600,
          },
          {
            id: "tt-3",
            title: "组会",
            role: "chore",
            start: DAY_START + 16 * 3600,
            end: DAY_START + 17 * 3600,
          },
          {
            id: "tt-4",
            title: "改 GameLife",
            role: "side",
            start: DAY_START + 20 * 3600,
            end: DAY_START + 21 * 3600,
          },
          {
            id: "tt-5",
            title: "回邮件",
            role: "chore",
            start: DAY_START + 10 * 3600,
            end: DAY_START + 10 * 3600 + 30 * 60,
          },
        ]
      : [],
  };
}

export const PREVIEW_WEEK: WeekView = {
  core: 1860,
  support: 40,
  admin: 180,
  side: 210,
  distraction: 90,
  away: 240,
  unobserved: 120,
  pendingReview: 0,
  wishes: WISHES,
  coinBalance: 1280,
  xpToday: 200,
  xpShopUnlocked: true,
  activeEntertainment: {
    name: "出门散步",
    endsAt: WALK_ENDS_AT,
    remainingSecs: WALK_REMAIN_SECS,
  },
  endedEntertainment: null,
  redemptions: [
    {
      id: "r-walk-now",
      name: "出门散步",
      ts: NOW_SECS - 7 * 60 + 20,
      durationMinutes: 20,
      kind: "xp",
      spent: 40,
      status: "进行中",
    },
    {
      id: "r1",
      name: "出门散步",
      ts: DAY_START - 86400 + 19 * 3600,
      durationMinutes: 20,
      kind: "xp",
      spent: 40,
      status: "已结束",
    },
  ],
  byDay: Array.from({ length: 7 }, (_, i) => ({
    day: addDaysIso(mondayOf(TODAY), i),
    core: [280, 360, 240, 420, 390, 180, 90][i]!,
    side: [20, 45, 30, 15, 60, 40, 10][i]!,
    chore: [40, 20, 35, 25, 15, 50, 0][i]!,
  })),
  byHour: Array.from({ length: 24 }, (_, hour) => ({
    hour,
    core: hour >= 9 && hour <= 17 ? 42 * 60 : hour === 8 || hour === 18 ? 18 * 60 : 0,
    observed: hour >= 8 && hour <= 18 ? 50 * 60 : 0,
  })),
  coreLabel: "主线 31h",
  creditedTodayMinutes: Math.floor(CREDITED_SECONDS / 60),
  coreHours: 31,
  wowCoreDeltaMinutes: 48,
  distractionObservedRatio: 0.08,
  pendingOverResolved: 0,
  daysGe6h: 4,
  daysGe8h: 2,
};

function previewDayHours(
  dayIndex: number,
  isFuture: boolean,
  isWeekend: boolean,
  creditedCore: number,
): string[] {
  const hours = Array.from({ length: 24 }, () => "");
  if (isFuture) return hours;
  if (isWeekend && creditedCore === 0) {
    hours[10] = "away";
    hours[11] = "unobserved";
    return hours;
  }
  if (isWeekend) {
    hours[9] = "side";
    hours[10] = "side";
    hours[11] = "admin";
    hours[14] = "entertainment";
    hours[15] = "away";
    return hours;
  }
  hours[7] = "away";
  hours[8] = "core";
  hours[9] = "core";
  hours[10] = dayIndex % 5 === 0 ? "support" : "core";
  hours[11] = "core";
  hours[12] = "admin";
  hours[13] = "core";
  hours[14] = "core";
  hours[15] = dayIndex % 4 === 0 ? "pending" : "core";
  hours[16] = "core";
  hours[17] = "side";
  hours[18] = "admin";
  hours[20] = "entertainment";
  hours[21] = "away";
  hours[22] = "unobserved";
  return hours;
}

export function previewMonth(year: number, month: number): MonthReportView {
  const n = daysInMonth(year, month);
  const days = Array.from({ length: n }, (_, i) => {
    const day = `${year}-${pad(month)}-${pad(i + 1)}`;
    const d = atLocalMidnight(day);
    const weekday = d.getDay();
    const isWeekend = weekday === 0 || weekday === 6;
    const isFuture = day > TODAY;
    let creditedCore = 0;
    if (!isFuture && !isWeekend) {
      creditedCore = 18000 + ((i * 137) % 7200);
    } else if (!isFuture && isWeekend) {
      creditedCore = i % 2 === 0 ? 0 : 5400;
    }
    return {
      day,
      creditedCore,
      isWeekend,
      isFuture,
      hours: previewDayHours(i, isFuture, isWeekend, creditedCore),
    };
  });
  return {
    days,
    activity: {
      core: 42000,
      support: 1200,
      admin: 3600,
      side: 4800,
      distraction: 1800,
      away: 5400,
      unobserved: 2400,
    },
    coinsEarned: 420,
    coinsSpent: 96,
    xpEarned: 3800,
    goldDays: 3,
    freezeCount: 1,
    completedDays: 16,
  };
}

export function previewRhythm(kind: "week" | "month" = "week"): RhythmReportView {
  const startHours =
    kind === "month" ? monthStartHours() : weekStartHours();
  return {
    startHours,
    rate6h: 0.72,
    rate8h: 0.41,
    distractionRunCount: 3,
    distractionRunSlots: 5,
    peakHours: [9, 10, 11, 14, 15, 16],
  };
}

function previewDayBands(hour: number | null): RhythmBand[] {
  if (hour == null) return [];
  const bands: RhythmBand[] = [];
  if (hour < 12) bands.push({ startHour: hour, endHour: 12, category: "core" });
  bands.push({ startHour: 12, endHour: 13, category: "away" });
  bands.push({ startHour: 13, endHour: 17, category: "core" });
  bands.push({ startHour: 17, endHour: 17.5, category: "side" });
  bands.push({ startHour: 21, endHour: 21.75, category: "distraction" });
  return bands;
}

function weekStartHours(): RhythmStartHour[] {
  const monday = mondayOf(TODAY);
  return Array.from({ length: 7 }, (_, i) => {
    const day = addDaysIso(monday, i);
    if (day > TODAY) return { day, hour: null, bands: [] };
    const weekend = atLocalMidnight(day).getDay() % 6 === 0;
    const hour = weekend ? null : 8;
    return { day, hour, bands: previewDayBands(hour) };
  });
}

function monthStartHours(): RhythmStartHour[] {
  const year = Number(TODAY.slice(0, 4));
  const month = Number(TODAY.slice(5, 7));
  const n = daysInMonth(year, month);
  const rows: RhythmStartHour[] = [];
  for (let i = 1; i <= n; i++) {
    const day = `${year}-${pad(month)}-${pad(i)}`;
    const weekday = atLocalMidnight(day).getDay();
    if (weekday === 0 || weekday === 6) continue;
    if (day > TODAY) {
      rows.push({ day, hour: null, bands: [] });
      continue;
    }
    const hour = i % 7 === 0 ? 12 : i % 4 === 0 ? 9 : 8;
    rows.push({ day, hour, bands: previewDayBands(hour) });
  }
  return rows;
}

export const PREVIEW_APPS: AppReportView = {
  apps: [
    { name: "Cursor", minutes: 1480, dominant: "core", listedAs: "主线", filed: true },
    { name: "Preview", minutes: 210, dominant: "side", listedAs: "支线", filed: true },
    { name: "Mail", minutes: 160, dominant: "admin", listedAs: "杂项", filed: true },
    { name: "YouTube", minutes: 95, dominant: "distraction", listedAs: "娱乐", filed: false },
    { name: "Finder", minutes: 40, dominant: "admin", listedAs: "杂项", filed: false },
  ],
  newcomers: ["Finder"],
  hosts: [
    { host: "github.com", minutes: 80, dominant: "core" },
    { host: "youtube.com", minutes: 95, dominant: "distraction" },
  ],
  protectedMinutes: 12,
};

export const PREVIEW_SETTINGS: AppSettings = {
  screenshotRetention: "none",
  sampleKeepDays: 7,
  loginAtStartup: true,
  trustedApps: [
    "Cursor",
    "Visual Studio Code",
    "Preview",
    "Zotero",
    "Google Chrome",
  ],
  distractionRules: ["bilibili.com", "youtube.com"],
  sideProjectRules: [],
  readingApps: ["Preview", "Zotero"],
  neverCaptureApps: [],
  primaryProvider: "opencode-go",
  fallbackProvider: "codex",
  visionProviders: [
    {
      id: "opencode-go",
      kind: "custom",
      baseUrl: "https://opencode.ai/zen/go/v1",
      model: "deepseek-v4-flash-vision-exp",
    },
    {
      id: "codex",
      kind: "codex",
      baseUrl: "",
      model: "gpt-5.4",
    },
  ],
  showRailLabels: true,
  todayTimelineMode: "gutter",
  silentStart: true,
  taskNotifications: false,
  adminApps: ["Mail", "Calendar"],
  categoryGuides: {
    mainline: "",
    side: "",
    admin: "",
    entertainment: "",
  },
  theme: "light",
  colorTheme: "default",
  ticktickEnabled: false,
  ticktickClientId: "",
  ticktickRedirectUri: "",
  ticktickProjectRoles: {},
  sync: {
    enabled: true,
    target: "webdav",
    url: "https://dav.jianguoyun.com/dav/",
    username: "demo@example.com",
    bucket: "",
    region: "auto",
    remotePath: "gamelife",
    intervalMinutes: 60,
    scope: "aggregate",
    keepSnapshots: 7,
    deviceLabel: "书房 Mac",
    settleGraceHours: 36,
  },
};

export const PREVIEW_SYNC_STATUS: SyncStatus = {
  enabled: true,
  target: "webdav",
  url: "https://dav.jianguoyun.com/dav/",
  scope: "aggregate",
  intervalMinutes: 60,
  lastAt: 1789372197,
  lastOk: true,
  lastError: "",
  snapshotBytes: 512 * 1024,
  deviceId: "9f3a1c2b4d5e6f70",
  devices: [
    {
      deviceId: "9f3a1c2b4d5e6f70",
      label: "书房 Mac",
      platform: "macos",
      lastSeen: 1789372197,
    },
    {
      deviceId: "1a2b3c4d5e6f7081",
      label: "Surface",
      platform: "windows",
      lastSeen: 1789365000,
    },
  ],
};

export const PREVIEW_TICKTICK_STATUS: TickTickStatus = {
  enabled: false,
  connected: false,
  clientId: "",
  projects: [],
  writeTargets: {},
  lastSyncAt: null,
  lastResult: "",
};

export const PREVIEW_KEY_STATUS: ProviderKeyStatus = {
  keys: { "opencode-go": true },
  codexLoggedIn: true,
};

export const PREVIEW_PERMISSIONS: PermissionStatus = {
  accessibility: true,
  screenRecording: true,
  processName: "GameLife",
  processPath: "/Applications/GameLife.app/Contents/MacOS/gamelife",
};

export const PREVIEW_OBSERVATION: ObservationStatus = {
  supported: true,
  backend: "macOS",
};

