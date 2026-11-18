import { arrayMove } from "@dnd-kit/sortable";

/**
 * Move the dragged queue entry to the drop position. Returns the original
 * array when either id is unknown or the row was dropped on itself, so the
 * caller can skip the persistence round-trip.
 */
export function moveQueueEntry<T extends { id: string }>(
  entries: T[],
  activeId: string,
  overId: string,
): T[] {
  const from = entries.findIndex((entry) => entry.id === activeId);
  const to = entries.findIndex((entry) => entry.id === overId);
  if (from < 0 || to < 0 || from === to) return entries;
  return arrayMove(entries, from, to);
}
