import { command, desktop } from "./api";
import { duration } from "./hooks";
import type { SessionRecord } from "./types";

export function formatRecord(record: SessionRecord, format: string): string {
  if (format === "json") return JSON.stringify(record, null, 2);
  if (format !== "txt" && format !== "md")
    throw new Error("Неизвестный формат экспорта");
  return (
    record.title +
    "\n" +
    record.source_language +
    " → " +
    record.target_language +
    "\n\n" +
    record.entries
      .map(
        (e) =>
          "[" +
          duration(e.timestamp_ms) +
          "]\n" +
          e.source +
          "\n" +
          (e.translation || e.error || "Перевод не завершён"),
      )
      .join("\n\n")
  );
}
export async function exportRecord(record: SessionRecord, format: string) {
  if (desktop)
    return command<string | null>("export_session", { id: record.id, format });
  const blob = new Blob([formatRecord(record, format)], {
    type:
      format === "json"
        ? "application/json;charset=utf-8"
        : "text/plain;charset=utf-8",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "babelhack-" + record.id + "." + format;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return true;
}
