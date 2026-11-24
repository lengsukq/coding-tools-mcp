export const isMacPlatform =
  typeof navigator !== "undefined" && /Macintosh|Mac OS X/.test(navigator.userAgent);

export const searchShortcutLabel = isMacPlatform ? "⌘K" : "Ctrl+K";
