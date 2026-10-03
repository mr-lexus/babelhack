import { useEffect, useState } from "react";
import { command, on, errorText } from "./api";
import type { ConfigStatus } from "./types";

export { useSession, useSessionSelector } from "./session-store";
export function useConfig() {
  const [config, setConfig] = useState<ConfigStatus | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    const refresh = () =>
      command<ConfigStatus>("get_config")
        .then((c) => {
          if (!disposed) {
            setConfig(c);
            setError("");
          }
        })
        .catch((e) => {
          if (!disposed) setError(errorText(e));
        });
    const sub = on("config-changed", refresh);
    void sub.then(refresh).catch((e) => setError(errorText(e)));
    return () => {
      disposed = true;
      void sub.then((fn) => fn()).catch(() => {});
    };
  }, []);
  return { config, error };
}
export function duration(ms: number) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return (
    Math.floor(seconds / 60)
      .toString()
      .padStart(2, "0") +
    ":" +
    (seconds % 60).toString().padStart(2, "0")
  );
}
