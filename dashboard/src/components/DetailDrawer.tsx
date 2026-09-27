import { ReactNode, useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useBodyScrollLock, useFocusTrap } from "../hooks/useFocusTrap";
import { CloseIcon } from "./Icons";

interface DetailDrawerProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
}

/** Side panel on desktop, full-screen sheet below the mobile breakpoint. */
export function DetailDrawer({ open, title, onClose, children }: DetailDrawerProps) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();

  useFocusTrap(ref, open, onClose);
  useBodyScrollLock(open);

  if (!open) return null;

  return (
    <>
      <div className="scrim" aria-hidden="true" />
      <div ref={ref} className="drawer" role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}>
        <header className="drawer-header">
          <h2 id={titleId}>{title}</h2>
          <button type="button" className="icon-button" aria-label={t("common.close")} onClick={onClose}>
            <CloseIcon />
          </button>
        </header>
        <div className="drawer-body">{children}</div>
      </div>
    </>
  );
}
