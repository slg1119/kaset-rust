import { expect, test } from "vitest";
import { safeImageUrl } from "./imageUrl";

test("keeps https artwork on YouTube image hosts", () => {
  for (const url of [
    "https://i.ytimg.com/vi/test-vid-01/hqdefault.jpg",
    "https://i9.ytimg.com/vi/test-vid-01/sddefault.jpg",
    "https://lh3.googleusercontent.com/test=w120-h120",
    "https://yt3.googleusercontent.com/test=s240",
    "https://yt3.ggpht.com/test=s88",
  ]) {
    expect(safeImageUrl(url)).toBe(url);
  }
});

test("upgrades protocol-relative URLs", () => {
  expect(safeImageUrl("//i.ytimg.com/vi/test/default.jpg")).toBe("https://i.ytimg.com/vi/test/default.jpg");
});

test("rejects other schemes, hosts, and junk", () => {
  for (const url of [
    null,
    "http://i.ytimg.com/vi/test/default.jpg",
    "https://example.com/a.jpg",
    "https://ytimg.com.example.com/a.jpg",
    "https://googleusercontent.com/a.jpg",
    "javascript:alert(1)",
    "not a url",
  ]) {
    expect(safeImageUrl(url)).toBe(null);
  }
});
