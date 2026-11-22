import { Fragment } from "react";
import type { ArtistRef } from "../bindings";

export type OpenArtist = (browseId: string, name: string) => void;

export function ArtistNames({ artists, onOpenArtist }: { artists: ArtistRef[]; onOpenArtist?: OpenArtist }) {
  return artists.map(({ name, browse_id }, index) => (
    <Fragment key={index}>
      {index > 0 && ", "}
      {browse_id && onOpenArtist ? (
        <button type="button" className="artist-link" aria-label={`Open ${name}`} onClick={() => onOpenArtist(browse_id, name)}>
          {name}
        </button>
      ) : name}
    </Fragment>
  ));
}
