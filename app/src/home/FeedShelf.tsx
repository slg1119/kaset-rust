import { type CSSProperties, useId, useRef } from "react";
import type { BrowseEndpoint, HomeSection, ShelfItem, Song } from "../bindings";
import { ChevronLeftIcon, ChevronRightIcon } from "../icons";
import { ArtistNames, type OpenArtist } from "../music/ArtistNames";
import { ItemCard } from "../music/ItemCard";
import { hasBrowsePage, isPlayable, itemKey, shapeOf, subtitleOf, toSong, type PageCard } from "../music/items";
import { MAX_QUEUE_LENGTH } from "../player/usePlayQueue";

export type CategoryItem = Extract<ShelfItem, { kind: "category" }>;

export function CategoryGrid({ title, items, onOpen }: { title: string; items: CategoryItem[]; onOpen: (item: CategoryItem) => void }) {
  const headingId = useId();
  return (
    <section className="shelf" aria-labelledby={headingId}>
      <div className="shelf-header"><h2 id={headingId}>{title}</h2></div>
      <ul className="category-grid">
        {items.map((item, index) => (
          <li key={`${index}-${item.browse_id}`}><CategoryTile item={item} onOpen={onOpen} /></li>
        ))}
      </ul>
    </section>
  );
}

function CategoryTile({ item, onOpen }: { item: CategoryItem; onOpen: (item: CategoryItem) => void }) {
  return (
    <button type="button" className="category-tile" style={item.color ? ({ "--tile-color": item.color } as CSSProperties) : undefined} onClick={() => onOpen(item)}>
      {item.title}
    </button>
  );
}

type ShelfProps = {
  section: HomeSection;
  onPlay: (songs: Song[], index: number) => Promise<void>;
  onOpenPage: (card: PageCard) => void;
  onOpenCategory: (item: CategoryItem) => void;
  onOpenArtist?: OpenArtist;
  onOpenMore?: (endpoint: BrowseEndpoint, title: string) => void;
  layout?: "quick-picks" | "carousel" | "rows" | "grid";
};

export function FeedShelf({
  section, onPlay, onOpenPage, onOpenCategory, onOpenArtist, onOpenMore, layout = "carousel",
}: ShelfProps) {
  const headingId = useId();
  const list = useRef<HTMLUListElement>(null);
  const songs = section.items.filter(isPlayable).map(toSong);
  const row = layout === "rows" || layout === "quick-picks";
  const className = layout === "quick-picks" ? "song-grid" : layout === "grid" ? "artist-items-grid" : layout;
  const more = section.more;
  let songIndex = 0;

  function playFrom(index: number) {
    const offset = index < MAX_QUEUE_LENGTH ? 0 : index;
    return onPlay(songs.slice(offset, offset + MAX_QUEUE_LENGTH), index - offset);
  }

  function page(direction: 1 | -1) {
    const element = list.current;
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    element?.scrollBy?.({ left: direction * element.clientWidth, behavior: reducedMotion ? "auto" : "smooth" });
  }

  const showHeader = layout === "carousel" || layout === "quick-picks";
  return (
    <section className="shelf" aria-labelledby={showHeader ? headingId : undefined} aria-label={showHeader ? undefined : section.title}>
      {showHeader && <div className="shelf-header">
        <h2 id={headingId}>{section.title}</h2>
        <div className="shelf-nav">
          {onOpenMore && more && (
            <button type="button" className="ghost" aria-label={`See all ${section.title}`} onClick={() => onOpenMore(more, section.title)}>
              See all
            </button>
          )}
          {layout === "carousel" && (
            <>
              <button type="button" className="icon-button" aria-label={`Previous in ${section.title}`} title={`Previous in ${section.title}`} onClick={() => page(-1)}>
                <ChevronLeftIcon size={16} />
              </button>
              <button type="button" className="icon-button" aria-label={`Next in ${section.title}`} title={`Next in ${section.title}`} onClick={() => page(1)}>
                <ChevronRightIcon size={16} />
              </button>
            </>
          )}
        </div>
      </div>}
      <ul ref={list} className={className}>
        {section.items.map((item, index) => {
          const playable = isPlayable(item);
          const start = playable ? songIndex++ : -1;
          return (
            <li key={`${index}-${itemKey(item)}`}>
              {item.kind === "category" ? <CategoryTile item={item} onOpen={onOpenCategory} /> : (
                <ItemCard
                  layout={row ? "row" : "card"}
                  shape={shapeOf(item)}
                  title={item.title}
                  subtitle={subtitleOf(item)}
                  thumbnailUrl={item.thumbnail_url}
                  song={playable ? songs[start] : undefined}
                  onOpenArtist={onOpenArtist}
                  artistCredits={"artists" in item && onOpenArtist && item.artists.some(({ browse_id }) => browse_id) ?
                    <ArtistNames artists={item.artists} onOpenArtist={onOpenArtist} /> : undefined}
                  onOpen={hasBrowsePage(item) ? () => onOpenPage(item) : undefined}
                  onPlay={playable ? () => void playFrom(start) : undefined}
                />
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
