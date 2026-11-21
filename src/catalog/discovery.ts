import type { Series } from "./types";

/** Categorization is driven by actual provider metadata, never invented tags. */
export function discoveryGenres(items: readonly Series[]): string[] {
  const genres = new Set(items.map(item => item.genre.trim()).filter(Boolean));
  return Array.from(genres).sort((a, b) => a.localeCompare(b, "zh-CN"));
}

/** Category chips filter only items already loaded from the selected provider(s). */
export function discoveryItems(items: readonly Series[], genre: string): Series[] {
  return genre === "all" ? [...items] : items.filter(item => item.genre.trim() === genre);
}

/** Prefer user-controlled content for the prominent hero, not an invented popularity score. */
export function discoveryHero(items: readonly Series[]): Series | null {
  return items.find(item => item.providerId === "manifest")
    ?? items.find(item => item.providerId === "local")
    ?? items[0]
    ?? null;
}

export function discoverySourceLabel(id: string, names: ReadonlyMap<string, string>): string {
  return names.get(id) ?? (id === "manifest" ? "自有媒体清单"
    : id === "local" ? "本地媒体" : id === "demo" ? "演示内容" : "其他来源");
}
