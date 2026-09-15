import { ref } from "vue";

export type ToastKind = "info" | "success" | "warning" | "error";

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface Toast {
  id: string;
  title?: string;
  message: string;
  kind: ToastKind;
  action?: ToastAction;
}

export interface ToastOptions {
  title?: string;
  kind?: ToastKind;
  /** Auto-dismiss after ms; 0 keeps the toast until dismissed manually. */
  duration?: number;
  action?: ToastAction;
}

export const toasts = ref<Toast[]>([]);
const timers = new Map<string, ReturnType<typeof setTimeout>>();

function nextId(): string {
  return crypto.randomUUID();
}

export function showToast(message: string, options: ToastOptions = {}): string {
  const toast: Toast = {
    id: nextId(),
    message,
    title: options.title,
    kind: options.kind ?? "info",
    action: options.action,
  };

  toasts.value = [...toasts.value, toast];

  const duration = options.duration ?? 5000;
  if (duration > 0) {
    const timer = setTimeout(() => dismissToast(toast.id), duration);
    timers.set(toast.id, timer);
  }

  return toast.id;
}

export function dismissToast(id: string): void {
  const timer = timers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    timers.delete(id);
  }
  toasts.value = toasts.value.filter((item) => item.id !== id);
}
