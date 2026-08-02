import React, { useRef, useImperativeHandle, forwardRef } from "react";
import { useTranslation } from "react-i18next";
import { ModalSurface } from "./ModalSurface";

export type DialogResponse = "save" | "discard" | "cancel";

export interface ConfirmOptions {
  cancelLabel?: string;
  confirmLabel?: string;
}

export interface DialogHandle {
  confirm: (message: string, title?: string, options?: ConfirmOptions) => Promise<boolean>;
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
  const { t, i18n } = useTranslation();
  const [open, setOpen] = React.useState(false);
  const [type, setType] = React.useState<"confirm" | "alert" | "ask">("confirm");
  const [message, setMessage] = React.useState("");
  const [titleKey, setTitleKey] = React.useState("dialog.title");
  const [confirmLabels, setConfirmLabels] = React.useState<Required<ConfirmOptions>>({
    cancelLabel: "dialog.cancel",
    confirmLabel: "dialog.confirm",
  });
  const resolveRef = useRef<(v?: boolean) => void>(() => {});
  const resolveAskRef = useRef<(v: DialogResponse) => void>(() => {});
  const overlayRef = useRef<HTMLDivElement>(null);
  const initialFocusRef = useRef<HTMLButtonElement>(null);

  useImperativeHandle(ref, () => ({
    async confirm(message: string, title = "dialog.title", options?: ConfirmOptions) {
      setType("confirm");
      setMessage(message);
      setTitleKey(title);
      setConfirmLabels({
        cancelLabel: options?.cancelLabel ?? "dialog.cancel",
        confirmLabel: options?.confirmLabel ?? "dialog.confirm",
      });
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

  const closeDialog = (result?: boolean | DialogResponse) => {
    setOpen(false);
    if (type === "ask") {
      resolveAskRef.current(
        result === "save" || result === "discard" ? result : "cancel",
      );
    } else {
      resolveRef.current(typeof result === "boolean" ? result : false);
    }
  };

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === overlayRef.current) closeDialog();
  };

  if (!open) return null;

  const resolveText = (value: string) => (i18n.exists(value) ? t(value) : value);

  return (
    <div className="dialog-overlay" ref={overlayRef} onClick={handleOverlayClick}>
      <ModalSurface
        className={`dialog-box ${theme}`}
        role={type === "alert" ? "alertdialog" : "dialog"}
        labelledBy="promise-dialog-title"
        describedBy="promise-dialog-message"
        onEscape={() => closeDialog()}
        initialFocusRef={initialFocusRef}
      >
        <div id="promise-dialog-title" className="dialog-title">
          {resolveText(titleKey)}
        </div>
        <div id="promise-dialog-message" className="dialog-message">
          {resolveText(message)}
        </div>
        <div className="dialog-actions">
          {type === "ask" ? (
            <>
              <button
                ref={initialFocusRef}
                type="button"
                className="dialog-btn dialog-btn-cancel"
                onClick={() => closeDialog("cancel")}
              >
                {t("dialog.cancel")}
              </button>
              <button
                type="button"
                className="dialog-btn dialog-btn-discard"
                onClick={() => closeDialog("discard")}
              >
                {t("dialog.discard")}
              </button>
              <button
                type="button"
                className="dialog-btn dialog-btn-save"
                onClick={() => closeDialog("save")}
              >
                {t("dialog.save")}
              </button>
            </>
          ) : type === "confirm" ? (
            <>
              <button
                ref={initialFocusRef}
                type="button"
                className="dialog-btn dialog-btn-cancel"
                onClick={() => closeDialog(false)}
              >
                {resolveText(confirmLabels.cancelLabel)}
              </button>
              <button
                type="button"
                className="dialog-btn dialog-btn-ok"
                onClick={() => closeDialog(true)}
              >
                {resolveText(confirmLabels.confirmLabel)}
              </button>
            </>
          ) : (
            <button
              ref={initialFocusRef}
              type="button"
              className="dialog-btn dialog-btn-ok"
              onClick={() => closeDialog(true)}
            >
              {t("dialog.confirm")}
            </button>
          )}
        </div>
      </ModalSurface>
    </div>
  );
});
