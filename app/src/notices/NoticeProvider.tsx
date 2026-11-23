import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export type NoticeApi = {
  /** Shows `message` in the app's single notice area, replacing any earlier one. */
  report: (message: string) => void;
  dismiss: () => void;
};

const NoticeContext = createContext<NoticeApi | null>(null);
const NoticeTargetContext = createContext<{
  target: HTMLElement | null;
  setTarget: (target: HTMLElement | null) => void;
} | null>(null);
const MessageContext = createContext<string | null>(null);

/**
 * One app-wide place for action errors (play, sign out, player controls), so
 * the same failure is never shown twice. Screens keep their own load errors.
 */
export function NoticeProvider({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState<string | null>(null);
  const [target, setTarget] = useState<HTMLElement | null>(null);
  const location = useMemo(() => ({ target, setTarget }), [target]);
  const api = useMemo<NoticeApi>(
    () => ({ report: setMessage, dismiss: () => setMessage(null) }),
    [],
  );
  return (
    <NoticeContext value={api}>
      <NoticeTargetContext value={location}>
        <MessageContext value={message}>{children}</MessageContext>
      </NoticeTargetContext>
    </NoticeContext>
  );
}

export function useNotice(): NoticeApi {
  const api = useContext(NoticeContext);
  if (api === null) throw new Error("useNotice must be used inside <NoticeProvider>");
  return api;
}

/** Moves the single notice into an open modal so it stays visible and accessible. */
export function useNoticeTarget() {
  const location = useContext(NoticeTargetContext);
  return location?.setTarget;
}

export function NoticeBanner() {
  const message = useContext(MessageContext);
  const { dismiss } = useNotice();
  const target = useContext(NoticeTargetContext)?.target;
  if (message === null) return null;
  const banner = (
    <div className="notice" role="alert">
      <p>{message}</p>
      <button type="button" onClick={dismiss}>
        Dismiss
      </button>
    </div>
  );
  return target ? createPortal(banner, target) : banner;
}
