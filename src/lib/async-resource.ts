export interface ResourceState<T> {
    value: T | null;
    updatedAt: number;
    loading: boolean;
    error: string | null;
}
/** Shared request ownership; invalidation isolates an earlier in-flight generation. */
export function createResource<T>(ttlMs = 3000, changed: () => void = () => { }, now = Date.now) {
    const state: ResourceState<T> = { value: null, updatedAt: 0, loading: false, error: null };
    let flight: Promise<T> | null = null;
    let generation = 0;
    function invalidate() { generation++; flight = null; state.updatedAt = 0; state.loading = false; changed(); }
    function set(value: T) { invalidate(); state.value = value; state.updatedAt = now(); state.error = null; changed(); }
    function get(loader: () => Promise<T>, force = false): Promise<T> {
        if (flight)
            return flight;
        if (!force && state.value !== null && !state.error && now() - state.updatedAt < ttlMs)
            return Promise.resolve(state.value);
        const id = generation;
        state.loading = true;
        changed();
        const request = Promise.resolve().then(loader).then(value => {
            if (id === generation) {
                state.value = value;
                state.updatedAt = now();
                state.error = null;
            }
            return value;
        }).catch(error => { if (id === generation)
            state.error = String(error); throw error; })
            .finally(() => { if (id === generation) {
            state.loading = false;
            flight = null;
            changed();
        } });
        flight = request;
        return request;
    }
    return { state, get, set, invalidate, stale: () => state.value !== null && (state.error !== null || now() - state.updatedAt >= ttlMs) };
}
