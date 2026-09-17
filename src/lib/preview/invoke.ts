import {
  PREVIEW_APPS,
  PREVIEW_KEY_STATUS,
  PREVIEW_OBSERVATION,
  PREVIEW_PERMISSIONS,
  PREVIEW_SETTINGS,
  PREVIEW_SYNC_STATUS,
  PREVIEW_TODAY,
  PREVIEW_WEEK,
  previewDayView,
  previewMonth,
  previewRhythm,
  PREVIEW_TASK_BOARD,
} from "./fixtures";

export async function previewInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  switch (cmd) {
    case "get_today":
      return PREVIEW_TODAY as T;
    case "list_task_board":
      return PREVIEW_TASK_BOARD as T;
    case "get_day_view":
      return previewDayView(String(args?.day ?? PREVIEW_TODAY.day)) as T;
    case "get_week":
      return PREVIEW_WEEK as T;
    case "get_month_report":
      return previewMonth(
        Number(args?.year ?? new Date().getFullYear()),
        Number(args?.month ?? new Date().getMonth() + 1),
      ) as T;
    case "get_rhythm_report":
      return previewRhythm() as T;
    case "get_app_report":
      return PREVIEW_APPS as T;
    case "get_settings":
      return PREVIEW_SETTINGS as T;
    case "provider_key_status":
      return PREVIEW_KEY_STATUS as T;
    case "has_api_key":
      return false as T;
    case "get_permission_status":
      return PREVIEW_PERMISSIONS as T;
    case "observation_status":
      return PREVIEW_OBSERVATION as T;
    case "save_settings": {
      const next = args?.settings;
      if (next && typeof next === "object") {
        Object.assign(PREVIEW_SETTINGS, next);
      }
      return undefined as T;
    }
    case "set_api_key":
    case "set_provider_api_key":
    case "set_quests":
    case "review_slot":
    case "report_misclassification":
    case "redeem":
    case "create_wish":
    case "update_wish":
    case "archive_wish":
    case "end_today":
    case "sync_status":
    case "sync_now_cmd":
      return PREVIEW_SYNC_STATUS as T;
    case "sync_list_devices":
      return PREVIEW_SYNC_STATUS.devices as unknown as T;
    case "sync_test_connection":
      return "连接正常，可写入 gamelife" as unknown as T;
    case "sync_set_credentials":
    case "sync_restore":
      return undefined as T;
    case "freeze":
    case "request_screen_recording":
    case "open_privacy_settings":
      return undefined as T;
    case "parse_task_line": {
      const line = String(args?.line ?? "");
      const currentListId =
        typeof args?.currentListId === "string"
          ? args.currentListId
          : (PREVIEW_TASK_BOARD.lists.find((list) => list.role === "mainline")
              ?.id ?? "list-mainline");
      const tag = line.match(/#(\S+)/)?.[1] ?? "";
      const tagged = PREVIEW_TASK_BOARD.lists.find(
        (list) =>
          list.name === tag ||
          (tag === "主线" && list.role === "mainline") ||
          (tag === "支线" && list.role === "side") ||
          (tag === "杂项" && list.role === "chore") ||
          ((tag === "长期" || tag === "长期规划") && list.role === "longterm"),
      );
      return {
        title: line.replace(/#\S+/g, "").replace(/，/g, " ").trim(),
        listId: tagged?.id ?? currentListId,
        start: null,
        end: null,
        parseOk: false,
        spans: [],
      } as T;
    }
    case "upsert_task": {
      const task = args?.task as
        | {
            id: string;
            listId: string;
            title: string;
            done: boolean;
            start: number | null;
            end: number | null;
            range: string | null;
            sort?: number;
            repeat?: string;
            remindOffsets?: number[];
            notes?: string;
          }
        | undefined;
      if (task) {
        const row = {
          ...task,
          sort: task.sort ?? PREVIEW_TASK_BOARD.tasks.length,
          repeat: task.repeat ?? "none",
          remindOffsets: task.remindOffsets ?? [],
          notes: task.notes ?? "",
        };
        const i = PREVIEW_TASK_BOARD.tasks.findIndex((t) => t.id === task.id);
        if (i >= 0) PREVIEW_TASK_BOARD.tasks[i] = row;
        else PREVIEW_TASK_BOARD.tasks.push(row);
      }
      return undefined as T;
    }
    case "toggle_task_done": {
      const id = String(args?.id ?? "");
      const done = Boolean(args?.done);
      const task = PREVIEW_TASK_BOARD.tasks.find((row) => row.id === id);
      if (task) task.done = done;
      return undefined as T;
    }
    case "create_list": {
      const name = String(args?.name ?? "").trim();
      const role = String(args?.role ?? "side");
      const created = {
        id: `list-preview-${Date.now()}`,
        name,
        sort: PREVIEW_TASK_BOARD.lists.length,
        role,
      };
      PREVIEW_TASK_BOARD.lists.push(created);
      return created as T;
    }
    case "rename_list": {
      const list = PREVIEW_TASK_BOARD.lists.find(
        (row) => row.id === String(args?.id ?? ""),
      );
      if (list) list.name = String(args?.name ?? list.name);
      return undefined as T;
    }
    case "delete_list": {
      const id = String(args?.id ?? "");
      PREVIEW_TASK_BOARD.lists = PREVIEW_TASK_BOARD.lists.filter(
        (row) => row.id !== id,
      );
      return undefined as T;
    }
    case "delete_task": {
      const id = String(args?.id ?? "");
      PREVIEW_TASK_BOARD.tasks = PREVIEW_TASK_BOARD.tasks.filter(
        (row) => row.id !== id,
      );
      return undefined as T;
    }
    case "move_task": {
      const task = PREVIEW_TASK_BOARD.tasks.find(
        (row) => row.id === String(args?.id ?? ""),
      );
      if (task) task.listId = String(args?.listId ?? task.listId);
      return undefined as T;
    }
    case "reschedule_task": {
      const task = PREVIEW_TASK_BOARD.tasks.find(
        (row) => row.id === String(args?.id ?? ""),
      );
      if (task) {
        const start = args?.start;
        const end = args?.end;
        task.start = typeof start === "number" ? start : null;
        task.end = typeof end === "number" ? end : null;
        if (task.start == null && task.end == null) {
          task.repeat = "none";
          task.remindOffsets = [];
        }
      }
      return undefined as T;
    }
    case "reorder_task": {
      const task = PREVIEW_TASK_BOARD.tasks.find(
        (row) => row.id === String(args?.id ?? ""),
      );
      if (task) {
        task.listId = String(args?.listId ?? task.listId);
        if (typeof args?.sort === "number") task.sort = args.sort;
      }
      return undefined as T;
    }
    case "reorder_list": {
      const list = PREVIEW_TASK_BOARD.lists.find(
        (row) => row.id === String(args?.id ?? ""),
      );
      if (list && typeof args?.sort === "number") list.sort = args.sort;
      PREVIEW_TASK_BOARD.lists.sort((a, b) => a.sort - b.sort || a.id.localeCompare(b.id));
      return undefined as T;
    }
    case "duplicate_task": {
      const src = PREVIEW_TASK_BOARD.tasks.find(
        (row) => row.id === String(args?.id ?? ""),
      );
      if (!src) return undefined as T;
      const copy = {
        ...src,
        id: `task-preview-${Date.now()}`,
        done: false,
        sort: Math.max(0, ...PREVIEW_TASK_BOARD.tasks.map((t) => t.sort)) + 1,
      };
      PREVIEW_TASK_BOARD.tasks.push(copy);
      return copy as T;
    }
    default:
      console.warn(`[preview] unhandled invoke: ${cmd}`);
      return undefined as T;
  }
}
