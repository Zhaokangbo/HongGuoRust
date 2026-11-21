export interface Series {
  id: string;
  title: string;
  description: string;
  genre: string;
  badge: string;
  artwork: "forest" | "city" | "studio";
  episodeCount: number;
  providerId: string;
}

export interface CatalogProvider {
  id: string;
  name: string;
  supportsSearch: boolean;
  supportsDetail: boolean;
  supportsPlay: boolean;
}

export interface CatalogSearchPage {
  items: Series[];
  total: number;
  nextOffset: number | null;
}

export interface Episode {
  id: string;
  number: number;
  title: string;
  runtimeLabel: string;
}

export interface SeriesDetail {
  series: Series;
  episodes: Episode[];
}

export interface LocalMedia {
  id: number;
  title: string;
  filename: string;
  available: boolean;
  positionSec: number;
  durationSec: number | null;
  lastPlayedMs: number | null;
  completed: boolean;
  favorite: boolean;
}
