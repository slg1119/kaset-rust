import type { ReactNode } from "react";
import type { Song } from "../bindings";
import type { OpenArtist } from "./ArtistNames";
import { Artwork } from "./Artwork";
import { SongMenu } from "./SongMenu";

type Props = {
  title: string;
  subtitle: string | null;
  thumbnailUrl: string | null;
  artistCredits?: ReactNode;
  /** `card`: artwork above the text (carousels); `row`: 44 px artwork beside it (lists). */
  layout: "card" | "row";
  /** Artists are round. */
  shape?: "square" | "round";
  /** Plays the item; without it the item is shown but not interactive. */
  onPlay?: () => void;
  onOpen?: () => void;
  song?: Song;
  onOpenArtist?: OpenArtist;
};

export function ItemCard({
  title, subtitle, thumbnailUrl, layout, shape = "square", artistCredits, onPlay, onOpen, song, onOpenArtist,
}: Props) {
  const body = (
    <>
      <Artwork url={thumbnailUrl} size={layout === "card" ? "fill" : 44} shape={shape} />
      <span className="item-text">
        <span className="item-title">{title}</span>
        {subtitle && <span className="item-subtitle">{artistCredits ?? subtitle}</span>}
      </span>
    </>
  );
  const className = `item item-${layout}`;
  const onClick = onOpen ?? onPlay;
  const label = itemActionLabel(onOpen ? "Open" : "Play", title, subtitle);
  if (song) {
    return (
      <SongMenu song={song} className={`${className} item-with-links`} onOpenArtist={onOpenArtist}>
        {onClick && <button type="button" className="item-action" aria-label={label} onClick={onClick} />}
        {body}
      </SongMenu>
    );
  }
  if (onClick === undefined) return <div className={className}>{body}</div>;
  if (artistCredits) {
    return (
      <div className={`${className} item-with-links`}>
        <button type="button" className="item-action" aria-label={label} onClick={onClick} />
        {body}
      </div>
    );
  }
  return (
    <button
      type="button"
      className={className}
      aria-label={label}
      onClick={onClick}
    >
      {body}
    </button>
  );
}

export function itemActionLabel(action: "Open" | "Play", title: string, subtitle: string | null) {
  return `${action} ${[title, subtitle].filter(Boolean).join(", ")}`;
}
