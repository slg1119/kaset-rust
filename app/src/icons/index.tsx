import type { ReactNode } from "react";

type IconProps = { size?: number };

function Icon({ size = 20, filled = false, children }: IconProps & { filled?: boolean; children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke={filled ? "none" : "currentColor"}
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export function PlayIcon(props: IconProps) {
  return (
    <Icon {...props} filled>
      <path d="M7 4.8a1 1 0 0 1 1.53-.85l11.2 7.2a1 1 0 0 1 0 1.7l-11.2 7.2A1 1 0 0 1 7 19.2Z" />
    </Icon>
  );
}

export function PauseIcon(props: IconProps) {
  return (
    <Icon {...props} filled>
      <rect x="6" y="4" width="4.5" height="16" rx="1.25" />
      <rect x="13.5" y="4" width="4.5" height="16" rx="1.25" />
    </Icon>
  );
}

export function PreviousIcon(props: IconProps) {
  return (
    <Icon {...props} filled>
      <rect x="2" y="5" width="2" height="14" rx="1" />
      <path d="M12.35 5.5a1 1 0 0 1 1.65.76v4.24l6.35-5a1 1 0 0 1 1.65.76v11.48a1 1 0 0 1-1.65.76L14 13.5v4.24a1 1 0 0 1-1.65.76l-7-5.74a1 1 0 0 1 0-1.52Z" />
    </Icon>
  );
}

export function NextIcon(props: IconProps) {
  return (
    <Icon {...props} filled>
      <rect x="20" y="5" width="2" height="14" rx="1" />
      <path d="M11.65 5.5A1 1 0 0 0 10 6.26v4.24l-6.35-5A1 1 0 0 0 2 6.26v11.48a1 1 0 0 0 1.65.76L10 13.5v4.24a1 1 0 0 0 1.65.76l7-5.74a1 1 0 0 0 0-1.52Z" />
    </Icon>
  );
}

export function ShuffleIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M3 6h2c2 0 3.2 1 4.6 2.7M14.4 15.3C15.8 17 17 18 19 18h2m-3-3 3 3-3 3M3 18h2c6 0 8-12 14-12h2m-3-3 3 3-3 3" />
    </Icon>
  );
}

export function RepeatIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M3 10a4 4 0 0 1 4-4h14m-3-3 3 3-3 3m3 5a4 4 0 0 1-4 4H3m3-3-3 3 3 3" />
    </Icon>
  );
}

export function RepeatOneIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M3 10a4 4 0 0 1 4-4h14m-3-3 3 3-3 3m3 5a4 4 0 0 1-4 4H3m3-3-3 3 3 3M11 10.5l1.5-1V15" />
    </Icon>
  );
}

export function QueueIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M3 6h18M3 12h10M3 18h10" />
      <path d="M16 12.5v7l5.5-3.5Z" fill="currentColor" stroke="none" />
    </Icon>
  );
}

export function HomeIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m3 10 9-7 9 7M5 9v10a1 1 0 0 0 1 1h4v-5h4v5h4a1 1 0 0 0 1-1V9" />
    </Icon>
  );
}

export function SearchIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="m15.1 15.1 4.9 4.9" />
    </Icon>
  );
}

export function RefreshIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M20 11a8 8 0 1 0-.6 4M20 5v6h-6" />
    </Icon>
  );
}

export function ChevronLeftIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m15 5-7 7 7 7" />
    </Icon>
  );
}

export function ChevronRightIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="m9 5 7 7-7 7" />
    </Icon>
  );
}

export function SignOutIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M11 4H5a1 1 0 0 0-1 1v14a1 1 0 0 0 1 1h6m-1-8h11m-4-4 4 4-4 4" />
    </Icon>
  );
}

export function MusicNoteIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M10 17V6l9-2v11" />
      <ellipse cx="7.5" cy="17.5" rx="2.5" ry="2" />
      <ellipse cx="16.5" cy="15.5" rx="2.5" ry="2" />
    </Icon>
  );
}

export function CompassIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="m15.5 8.5-2 5-5 2 2-5Z" />
    </Icon>
  );
}

export function ChartBarsIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M4 20h16M7 16v-4m5 4V8m5 8V4" />
    </Icon>
  );
}

export function PaletteIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 3a9 9 0 0 0 0 18h1a1.5 1.5 0 0 0 1.1-2.5 2 2 0 0 1 1.5-3.5H17a4 4 0 0 0 4-4c0-4.4-4-8-9-8Z" />
      <path d="M7.5 11h.01M10 7.5h.01m4.5 0h.01M17.5 11h.01" />
    </Icon>
  );
}

export function SparkleIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 2.5 14.6 9.4 21.5 12l-6.9 2.6L12 21.5l-2.6-6.9L2.5 12l6.9-2.6Z" />
    </Icon>
  );
}

export function ClockIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </Icon>
  );
}

export function PodcastIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="9" y="3" width="6" height="12" rx="3" />
      <path d="M5 11v1a7 7 0 0 0 14 0v-1M12 19v3m-3 0h6" />
    </Icon>
  );
}
