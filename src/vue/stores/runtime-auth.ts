import { shallowRef } from "vue";
import { defineStore } from "pinia";
import type { ChatGPTAuthAdapter, ChatGPTUser } from "../runtime/auth";

const defaultAdapter: ChatGPTAuthAdapter = {
  getUser: () => null,
};

/** Pinia setup store for the trusted ChatGPT identity/navigation bridge. */
export const useRuntimeAuthStore = defineStore("runtime-auth", () => {
  const adapter = shallowRef<ChatGPTAuthAdapter>(defaultAdapter);

  const setAdapter = (nextAdapter: ChatGPTAuthAdapter | null): (() => void) => {
    const previous = adapter.value;
    const next = nextAdapter ?? defaultAdapter;
    adapter.value = next;
    return () => {
      if (adapter.value === next) {
        adapter.value = previous;
      }
    };
  };

  const getUser = (): ChatGPTUser | null | Promise<ChatGPTUser | null> =>
    adapter.value.getUser();

  const redirect = async (path: string): Promise<void> => {
    if (adapter.value.redirect) {
      await adapter.value.redirect(path);
    } else if (typeof window !== "undefined") {
      window.location.assign(path);
    }
  };

  return { adapter, setAdapter, getUser, redirect };
});
