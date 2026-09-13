export const SETTINGS_CHANGED_EVENT = "gamelife:settings-changed";

export function notifySettingsChanged(target: EventTarget = window): void {
  target.dispatchEvent(new Event(SETTINGS_CHANGED_EVENT));
}
