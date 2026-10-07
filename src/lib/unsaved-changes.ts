import { computed, onUnmounted, ref, type Ref } from "vue";
interface Draft {
    label: string;
    dirty: Ref<boolean>;
    save: () => Promise<boolean | void>;
}
const drafts = new Map<symbol, Draft>();
const revision = ref(0);
export const unsavedDialogOpen = ref(false);
export const unsavedError = ref("");
export const hasUnsavedChanges = computed(() => { revision.value; return [...drafts.values()].some(d => d.dirty.value); });
export const unsavedLabels = computed(() => { revision.value; return [...drafts.values()].filter(d => d.dirty.value).map(d => d.label); });
let resolveNavigation: ((leave: boolean) => void) | null = null;
export function useUnsavedChanges(label: string, dirty: Ref<boolean>, save: Draft["save"]) {
    const key = Symbol(label);
    drafts.set(key, { label, dirty, save });
    revision.value++;
    onUnmounted(() => { drafts.delete(key); revision.value++; });
}
export async function confirmUnsavedChanges(): Promise<boolean> {
    if (!hasUnsavedChanges.value)
        return true;
    if (resolveNavigation)
        return false;
    unsavedError.value = "";
    unsavedDialogOpen.value = true;
    return new Promise(resolve => { resolveNavigation = resolve; });
}
export async function resolveUnsavedChanges(action: "save" | "discard" | "cancel") {
    if (action === "save") {
        try {
            for (const draft of drafts.values()) {
                if (draft.dirty.value && await draft.save() === false) {
                    unsavedError.value = draft.label + " 保存失败。输入已保留，可以重试或取消离开查看字段错误。";
                    return;
                }
            }
        }
        catch (error) {
            unsavedError.value = String(error);
            return;
        }
        if (hasUnsavedChanges.value) {
            unsavedError.value = "仍有未保存的更改，请完成保存或放弃。";
            return;
        }
    }
    unsavedDialogOpen.value = false;
    resolveNavigation?.(action !== "cancel");
    resolveNavigation = null;
}
