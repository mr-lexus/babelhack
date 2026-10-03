import { useCallback, useSyncExternalStore } from "react";
import { command, on, errorText } from "./api";
import { EMPTY_STATE, type SessionState } from "./types";

// One transport subscription per window. Polls are recovery/level-meter updates;
// text is event-driven. Unchanged fields retain their identities across IPC JSON.
export class SessionStore {
  state: SessionState = EMPTY_STATE;
  error = "";
  listeners = new Set<() => void>();
  accept = (incoming: SessionState) => {
    const old = this.state;
    const next = { ...EMPTY_STATE, ...incoming };
    if ((next.started_at ?? 0) < (old.started_at ?? 0)) return;
    if (next.session_id === old.session_id && next.version < old.version)
      return;
    if (
      next.lines.length === old.lines.length &&
      next.lines.every((v, i) => v === old.lines[i])
    )
      next.lines = old.lines;
    if (next.session_id === old.session_id) {
      const previous = new Map(old.cues.map((c) => [c.id, c]));
      next.cues = next.cues.map((c) =>
        previous.get(c.id)?.text === c.text &&
        !!previous.get(c.id)?.untranslated === !!c.untranslated
          ? previous.get(c.id)!
          : c,
      );
      if (
        next.cues.length === old.cues.length &&
        next.cues.every((c, i) => c === old.cues[i])
      )
        next.cues = old.cues;
    }
    const changed = (Object.keys(next) as (keyof SessionState)[]).some(
      (k) => !Object.is(next[k], old[k]),
    );
    if (!changed && !this.error) return;
    this.state = changed ? next : old;
    this.error = "";
    this.listeners.forEach((fn) => fn());
  };
}

export const sessionStore = new SessionStore();
let stop: (() => void) | undefined;
function connect() {
  let disposed = false;
  const fail = (e: unknown) => {
    if (disposed) return;
    sessionStore.error = errorText(e);
    sessionStore.listeners.forEach((fn) => fn());
  };
  const accept = (state: SessionState) => {
    if (!disposed) sessionStore.accept(state);
  };
  const sub = on<SessionState>("session-state", accept);
  const refresh = () =>
    command<SessionState>("get_session_state").then(accept).catch(fail);
  void sub.then(refresh).catch(fail);
  const timer = setInterval(
    refresh,
    document.documentElement.dataset.window === "overlay" ? 2000 : 600,
  );
  return () => {
    disposed = true;
    clearInterval(timer);
    void sub.then((fn) => fn()).catch(() => {});
  };
}
const subscribe = (fn: () => void) => {
  sessionStore.listeners.add(fn);
  if (!stop) stop = connect();
  return () => {
    sessionStore.listeners.delete(fn);
    if (!sessionStore.listeners.size) {
      stop?.();
      stop = undefined;
    }
  };
};
export function useSessionSelector<T>(selector: (state: SessionState) => T): T {
  // Selectors must return a primitive or an immutable field of the snapshot.
  const get = useCallback(() => selector(sessionStore.state), [selector]);
  return useSyncExternalStore(subscribe, get, get);
}
const identity = (s: SessionState) => s;
const getError = () => sessionStore.error;
export function useSession() {
  const state = useSessionSelector(identity);
  const error = useSyncExternalStore(subscribe, getError, getError);
  return { state, error };
}
