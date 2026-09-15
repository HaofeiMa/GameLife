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
} from "./fixtures";

export async function previewInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  switch (cmd) {
    case "get_today":
    case "list_tasks":
      return PREVIEW_TODAY as T;
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
    default:
      console.warn(`[preview] unhandled invoke: ${cmd}`);
      return undefined as T;
  }
}
