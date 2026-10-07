/** Serialize read/merge/write groups so concurrent saves do not replace each other. */
export function createWriteQueue() {
  let tail: Promise<unknown> = Promise.resolve();
  return function enqueue<T>(write: () => Promise<T>): Promise<T> {
    const pending = tail.then(write);
    tail = pending.catch(() => undefined);
    return pending;
  };
}

// Module ownership survives page unmounts while a save is in flight.
export const enqueueRuntimeSettingsWrite = createWriteQueue();
export const enqueueWorkspaceSettingsWrite = createWriteQueue();
