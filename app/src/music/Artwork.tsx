import { useState } from "react";
import { safeImageUrl } from "./imageUrl";

type Props = {
  url: string | null;
  /** Width and height in CSS pixels, or `fill` to fill the parent's width (square). */
  size: number | "fill";
  /** Artists use round artwork; everything else is square. */
  shape?: "square" | "round";
};

/**
 * Square or round artwork, or a neutral placeholder when there is no allowed
 * URL or the image fails to load. Decorative: titles carry the meaning.
 */
export function Artwork({ url, size, shape = "square" }: Props) {
  const src = safeImageUrl(url);
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const className = `artwork artwork-${shape}${size === "fill" ? " artwork-fill" : ""}`;
  const dimensions = size === "fill" ? undefined : { width: size, height: size };
  if (src === null || src === failedSrc) {
    return <span className={`${className} artwork-placeholder`} style={dimensions} aria-hidden="true" />;
  }
  return (
    <img
      className={className}
      src={src}
      {...dimensions}
      alt=""
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
      onError={() => setFailedSrc(src)}
    />
  );
}
