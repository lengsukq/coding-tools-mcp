/** Apply only fields changed within the form's ownership to a fresh server snapshot. */
export function mergeOwnedChanges<T extends object>(latest: T, draft: T, baseline: T, owned: ReadonlyArray<keyof T>): T {
    const result = { ...latest };
    for (const key of owned)
        if (JSON.stringify(draft[key]) !== JSON.stringify(baseline[key]))
            result[key] = draft[key];
    return result;
}
