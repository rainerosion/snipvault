import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface TitlebarProps {
  theme: "dark" | "light";
}

export function Titlebar({ theme }: TitlebarProps) {
  const { t } = useTranslation();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    win.isMaximized().then(setMaximized).catch(() => {});
    const unlisten = win.onResized(() => {
      win.isMaximized().then(setMaximized).catch(() => {});
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  const handleMinimize = () => getCurrentWindow().minimize();
  const handleMaximize = () => {
    const win = getCurrentWindow();
    if (maximized) {
      win.unmaximize();
    } else {
      win.maximize();
    }
  };
  const handleClose = () => getCurrentWindow().close();

  return (
    <div data-tauri-drag-region className={`titlebar ${theme}`}>
      <div className="titlebar-icon" data-tauri-drag-region>
        <img src="/icon-32.png" alt="SnipVault" width="16" height="16" />
      </div>
      <span className="titlebar-title" data-tauri-drag-region>{t("app.title")}</span>

      <div className="titlebar-controls">
        <button className="titlebar-btn minimize" onClick={handleMinimize} title={t("titlebar.minimize")}>
          <svg width="12" height="12" viewBox="0 0 12 12">
            <rect x="1" y="5.5" width="10" height="1" fill="currentColor"/>
          </svg>
        </button>
        <button className="titlebar-btn maximize" onClick={handleMaximize} title={maximized ? t("titlebar.restore") : t("titlebar.maximize")}>
          <svg width="12" height="12" viewBox="0 0 12 12">
            {maximized ? (
              <>
                <rect x="3" y="0" width="8" height="8" fill="none" stroke="currentColor" strokeWidth="1.2"/>
                <rect x="0" y="3" width="8" height="8" fill="none" stroke="currentColor" strokeWidth="1.2"/>
              </>
            ) : (
              <rect x="1" y="1" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1.2"/>
            )}
          </svg>
        </button>
        <button className="titlebar-btn close" onClick={handleClose} title={t("titlebar.close")}>
          <svg width="12" height="12" viewBox="0 0 12 12">
            <line x1="1" y1="1" x2="11" y2="11" stroke="currentColor" strokeWidth="1.4"/>
            <line x1="11" y1="1" x2="1" y2="11" stroke="currentColor" strokeWidth="1.4"/>
          </svg>
        </button>
      </div>
    </div>
  );
}
