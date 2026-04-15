import React, { useEffect, useRef, useImperativeHandle, forwardRef, useContext } from "react";
import { useTranslation } from "react-i18next";

export type DialogResponse = "save" | "discard" | "cancel";

export interface DialogHandle {
  confirm: (message: string, title?: string) => Promise<boolean>;
  alert: (message: string, title?: string) => Promise<void>;
  ask: (message: string, title?: string) => Promise<DialogResponse>;
}

export interface DialogProps {
  theme: "dark" | "light";
}

export const Dialog = forwardRef<DialogHandle, DialogProps>(function Dialog(
  { theme },
  ref
) {
  const { t } = useTranslation();
  const [open, setOpen] = React.useState(false);
  const [type, setType] = React.useState<"confirm" | "alert" | "ask">("confirm");
  const [message, setMessage] = React.useState("");
  const [titleKey, setTitleKey] = React.useState("dialog.title");
  const resolveRef = useRef<(v?: boolean) => void>(() => {});
  const resolveAskRef = useRef<(v: DialogResponse) => void>(() => {});
  const overlayRef = useRef<HTMLDivElement>(null);

  useImperativeHandle(ref, () => ({
    async confirm(message: string, title = "dialog.title") {
      setType("confirm");
      setMessage(message);
      setTitleKey(title);
      setOpen(true);
      return new Promise<boolean>((r) => {
        resolveRef.current = (v?: boolean) => r(v ?? false);
      });
    },
    async alert(message: string, title = "dialog.title") {
      setType("alert");
      setMessage(message);
      setTitleKey(title);
      setOpen(true);
      return new Promise<void>((r) => {
        resolveRef.current = () => r();
      });
    },
    async ask(message: string, title = "dialog.title") {
      setType("ask");
      setMessage(message);
      setTitleKey(title);
      setOpen(true);
      return new Promise<DialogResponse>((r) => {
        resolveAskRef.current = r;
      });
    },
  }));

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setOpen(false);
        if (type === "ask") resolveAskRef.current("cancel");
        else resolveRef.current(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, type]);

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === overlayRef.current) {
      setOpen(false);
      if (type === "ask") resolveAskRef.current("cancel");
      else resolveRef.current(false);
    }
  };

  if (!open) return null;

  return (
    <div className="dialog-overlay" ref={overlayRef} onClick={handleOverlayClick}>
      <div className={`dialog-box ${theme}`} onClick={(e) => e.stopPropagation()}>
        <div className="dialog-title">{t(titleKey)}</div>
        <div className="dialog-message">{t(message)}</div>
        <div className="dialog-actions">
          {type === "ask" ? (
            <>
              <button
                type="button"
                className="dialog-btn dialog-btn-cancel"
                onClick={() => { setOpen(false); resolveAskRef.current("cancel"); }}
              >
                {t("dialog.cancel")}
              </button>
              <button
                type="button"
                className="dialog-btn dialog-btn-discard"
                onClick={() => { setOpen(false); resolveAskRef.current("discard"); }}
              >
                {t("dialog.discard")}
              </button>
              <button
                type="button"
                className="dialog-btn dialog-btn-save"
                onClick={() => { setOpen(false); resolveAskRef.current("save"); }}
              >
                {t("dialog.save")}
              </button>
            </>
          ) : type === "confirm" ? (
            <>
              <button
                type="button"
                className="dialog-btn dialog-btn-cancel"
                onClick={() => { setOpen(false); resolveRef.current(false); }}
              >
                {t("dialog.cancel")}
              </button>
              <button
                type="button"
                className="dialog-btn dialog-btn-ok"
                onClick={() => { setOpen(false); resolveRef.current(true); }}
              >
                {t("dialog.confirm")}
              </button>
            </>
          ) : (
            <button
              type="button"
              className="dialog-btn dialog-btn-ok"
              onClick={() => { setOpen(false); resolveRef.current(); }}
            >
              {t("dialog.confirm")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
});
