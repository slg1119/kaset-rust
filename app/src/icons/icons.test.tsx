import { render } from "@testing-library/react";
import { expect, test } from "vitest";
import * as icons from ".";

const iconNames = [
  "PlayIcon", "PauseIcon", "PreviousIcon", "NextIcon", "ShuffleIcon",
  "RepeatIcon", "RepeatOneIcon", "QueueIcon", "HomeIcon", "SearchIcon",
  "RefreshIcon", "ChevronLeftIcon", "ChevronRightIcon", "SignOutIcon", "MusicNoteIcon",
] as const;

test.each(iconNames)("%s renders a decorative SVG at the requested size", (name) => {
  const Icon = icons[name];
  const { container, rerender } = render(<Icon />);
  const svg = container.querySelector("svg");
  expect(svg).toHaveAttribute("aria-hidden", "true");
  expect(svg).toHaveAttribute("focusable", "false");
  expect(svg).toHaveAttribute("viewBox", "0 0 24 24");
  expect(svg).toHaveAttribute("width", "20");
  expect(svg).toHaveAttribute("height", "20");

  rerender(<Icon size={32} />);
  expect(container.querySelector("svg")).toHaveAttribute("width", "32");
  expect(container.querySelector("svg")).toHaveAttribute("height", "32");
});
