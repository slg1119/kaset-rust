import { fireEvent, render } from "@testing-library/react";
import { expect, test } from "vitest";
import { Artwork } from "./Artwork";

test("shows allowed artwork and falls back to a placeholder when it fails", () => {
  const { container } = render(<Artwork url="https://i.ytimg.com/vi/test/default.jpg" size={48} />);
  const img = container.querySelector("img");
  expect(img).toHaveAttribute("src", "https://i.ytimg.com/vi/test/default.jpg");
  expect(img).toHaveAttribute("alt", "");
  fireEvent.error(img as HTMLImageElement);
  expect(container.querySelector("img")).toBeNull();
  expect(container.querySelector(".artwork-placeholder")).not.toBeNull();
});

test("never requests a disallowed URL", () => {
  const { container } = render(<Artwork url="http://example.com/a.jpg" size={48} />);
  expect(container.querySelector("img")).toBeNull();
});
