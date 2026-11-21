export interface Series {
  id: string;
  title: string;
  description: string;
  genre: string;
  badge: string;
  artwork: "forest" | "city" | "studio";
  episodeCount: number;
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
