export const APP_EVENTS = {
  TOGGLE_THEME: "coding-tools:toggle-theme",
  ADD_WORKSPACE: "coding-tools:add-workspace",
  OPEN_SEARCH: "coding-tools:open-search",
} as const;

export type AppEventName = (typeof APP_EVENTS)[keyof typeof APP_EVENTS];

export function dispatchAppEvent<T = undefined>(name: AppEventName, detail?: T): void {
  window.dispatchEvent(
    detail === undefined ? new CustomEvent(name) : new CustomEvent(name, { detail }),
  );
}
