import { useId, useLayoutEffect, useRef, type ReactNode } from "react";
import { useNoticeTarget } from "../notices/NoticeProvider";

export function ActionDialog({ title, busy, onClose, children }: {
  title: string;
  busy?: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  const headingId = useId();
  const target = useNoticeTarget();
  const dialog = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => {
    const element = dialog.current;
    if (!element) return;
    const restoreFocus = document.activeElement;
    element.showModal();
    return () => {
      element.close();
      if (restoreFocus instanceof HTMLElement && restoreFocus.isConnected) restoreFocus.focus();
    };
  }, []);

  return (
    <dialog ref={dialog} className="account-dialog" aria-labelledby={headingId}
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}>
      <h2 id={headingId}>{title}</h2>
      <div ref={target} />
      {children}
    </dialog>
  );
}
