import { createContext, useContext, useMemo, useState, type ReactNode } from "react";

export type NoticeApi = {
  /** Shows `message` in the app's single notice area, replacing any earlier one. */
  report: (message: string) => void;
  dismiss: () => void;
};

const NoticeContext = createContext<NoticeApi | null>(null);
const MessageContext = createContext<string | null>(null);

/**
 * One app-wide place for action errors (play, sign out, player controls), so
 * the same failure is never shown twice. Screens keep their own load errors.
 */
export function NoticeProvider({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState<string | null>(null);
  const api = useMemo<NoticeApi>(
    () => ({ report: setMessage, dismiss: () => setMessage(null) }),
    [],
  );
  return (
    <NoticeContext value={api}>
      <MessageContext value={message}>{children}</MessageContext>
    </NoticeContext>
  );
}

export function useNotice(): NoticeApi {
  const api = useContext(NoticeContext);
  if (api === null) throw new Error("useNotice must be used inside <NoticeProvider>");
  return api;
}

export function NoticeBanner() {
  const message = useContext(MessageContext);
  const { dismiss } = useNotice();
  if (message === null) return null;
  return (
    <div className="notice" role="alert">
      <p>{message}</p>
      <button type="button" onClick={dismiss}>
        Dismiss
      </button>
    </div>
  );
}
