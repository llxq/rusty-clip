const USER_AGENT = typeof navigator === "undefined" ? "" : navigator.userAgent;

export const HISTORY_UPDATED_EVENT = "clipboard-history-updated";
export const LAUNCHER_SHOWN_EVENT = "launcher-shown";

export const SHORTCUT_LABEL = USER_AGENT.includes("Mac")
  ? "Command + Shift + V"
  : "Ctrl + Shift + V";
