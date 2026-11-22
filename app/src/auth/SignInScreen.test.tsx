import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { SignInScreen } from "./SignInScreen";

test("opens the sign-in window", async () => {
  const signIn = vi.fn(() => null);
  mockCommands({ signIn });
  render(<SignInScreen expired={false} />);

  expect(screen.getByText(/sign in with your google account/i)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Sign in with Google" }));

  expect(signIn).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("explains an expired session", () => {
  render(<SignInScreen expired />);
  expect(screen.getByText(/your session expired/i)).toBeInTheDocument();
});

test("shows the error when the window cannot open", async () => {
  mockCommands({
    signIn: () => {
      throw { kind: "internal", message: "could not open the login window" };
    },
  });
  render(<SignInScreen expired={false} />);

  await userEvent.click(screen.getByRole("button", { name: "Sign in with Google" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("could not open the login window");
});
