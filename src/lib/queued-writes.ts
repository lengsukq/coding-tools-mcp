/** Serialize read/merge/write groups so concurrent saves do not replace each other. */
export function createWriteQueue() {
  let tail: Promise<unknown> = Promise.resolve();
  return function enqueue<T>(write: () => Promise<T>): Promise<T> {
    const pending = tail.then(write);
    tail = pending.catch(() => undefined);
    return pending;
  };
}
