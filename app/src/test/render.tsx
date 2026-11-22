import { QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { App } from "../App";
import { NoticeBanner, NoticeProvider } from "../notices/NoticeProvider";
import { createQueryClient } from "../query";

/** The whole app with a fresh query cache that never retries. */
export function renderApp() {
  return render(<App queryClient={createQueryClient({ retry: false })} />);
}

/** `ui` inside the providers the shell gives its screens, with the notice banner. */
export function renderWithProviders(ui: ReactElement) {
  return render(
    <QueryClientProvider client={createQueryClient({ retry: false })}>
      <NoticeProvider>
        <NoticeBanner />
        {ui}
      </NoticeProvider>
    </QueryClientProvider>,
  );
}
