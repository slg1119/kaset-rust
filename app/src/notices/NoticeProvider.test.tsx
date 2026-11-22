import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { NoticeBanner, NoticeProvider, useNotice } from "./NoticeProvider";

function Reporter() {
  const { report } = useNotice();
  return (
    <>
      <button type="button" onClick={() => report("first failure")}>
        First
      </button>
      <button type="button" onClick={() => report("second failure")}>
        Second
      </button>
    </>
  );
}

test("shows the latest notice once and dismisses it", async () => {
  render(
    <NoticeProvider>
      <NoticeBanner />
      <Reporter />
    </NoticeProvider>,
  );
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "First" }));
  await userEvent.click(screen.getByRole("button", { name: "Second" }));
  expect(screen.getAllByRole("alert")).toHaveLength(1);
  expect(screen.getByRole("alert")).toHaveTextContent("second failure");

  await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("useNotice outside the provider fails loudly", () => {
  expect(() => render(<Reporter />)).toThrow("useNotice must be used inside <NoticeProvider>");
});
