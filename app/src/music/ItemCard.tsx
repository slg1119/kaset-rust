import type { ReactNode } from "react";
import { Artwork } from "./Artwork";

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
};

export function ItemCard({ title, subtitle, thumbnailUrl, layout, shape = "square", artistCredits, onPlay, onOpen }: Props) {
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
  if (onClick === undefined) return <div className={className}>{body}</div>;
  const label = onOpen ? `Open ${title}` : subtitle ? `Play ${title}, ${subtitle}` : `Play ${title}`;
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
