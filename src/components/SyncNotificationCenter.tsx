import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  useSettings,
  type SyncNotification,
  type SyncNotificationCategory,
} from "../hooks/useSettings";
import { localizeCommandError } from "../utils/commandErrors";
import { ModalSurface } from "./ModalSurface";

interface SyncNotificationCenterProps {
  onClose: () => void;
  onSync: () => Promise<void>;
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function categoryKey(category: SyncNotificationCategory): string {
  return `syncNotifications.category.${category}`;
}

function sourceKey(source: SyncNotification["source"]): string {
  return `syncNotifications.source.${source}`;
}

function summary(notification: SyncNotification, t: ReturnType<typeof useTranslation>["t"]): string {
  if (notification.category === "restore_required") {
    return t("syncNotifications.restoreRequired");
  }
  if (notification.error_code) {
    return localizeCommandError(
      {
        code: notification.error_code,
        message: "",
        retryable: notification.retryable,
      },
      t,
    );
  }
  if (notification.category === "conflict") {
    return t("syncNotifications.conflictSummary", {
      count: notification.conflict_count ?? 0,
    });
  }
  if (notification.category === "failure") {
    return t("syncNotifications.failureSummary");
  }
  if (notification.category === "busy") {
    return t("syncNotifications.busySummary");
  }
  return t("syncNotifications.successSummary", {
    uploaded: notification.uploaded_count ?? 0,
    downloaded: notification.downloaded_count ?? 0,
    deleted: notification.deleted_count ?? 0,
  });
}

export function SyncNotificationCenter({
  onClose,
  onSync,
}: SyncNotificationCenterProps) {
  const { t } = useTranslation();
  const closeRef = useRef<HTMLButtonElement>(null);
  const {
    syncNotifications,
    notificationsLoading,
    notificationsError,
    reloadNotifications,
    markSyncNotificationRead,
    dismissSyncNotification,
    markAllSyncNotificationsRead,
  } = useSettings();

  useEffect(() => {
    void reloadNotifications().catch(() => {});
  }, [reloadNotifications]);

  const unreadCount = syncNotifications.filter(
    (notification) => notification.read_at === null,
  ).length;

  return (
    <div className="sync-notification-overlay">
      <ModalSurface
        className="sync-notification-dialog"
        labelledBy="sync-notification-title"
        describedBy="sync-notification-description"
        initialFocusRef={closeRef}
        onEscape={onClose}
      >
        <header className="sync-notification-header">
          <div>
            <p className="sync-notification-kicker">{t("syncNotifications.kicker")}</p>
            <h2 id="sync-notification-title">{t("syncNotifications.title")}</h2>
            <p id="sync-notification-description">
              {t("syncNotifications.description")}
            </p>
          </div>
          <button
            ref={closeRef}
            type="button"
            className="settings-close"
            onClick={onClose}
            aria-label={t("settings.close")}
          >
            ×
          </button>
        </header>

        <div className="sync-notification-actions">
          <button
            type="button"
            className="about-link"
            onClick={() => void markAllSyncNotificationsRead().catch(() => {})}
            disabled={unreadCount === 0}
          >
            {t("syncNotifications.markAllRead")}
          </button>
          <span>{t("syncNotifications.unread", { count: unreadCount })}</span>
        </div>

        <div className="sync-notification-list" aria-busy={notificationsLoading}>
          {notificationsLoading ? (
            <p className="sync-notification-empty" role="status">
              {t("syncNotifications.loading")}
            </p>
          ) : notificationsError ? (
            <p className="sync-notification-empty" role="alert">
              {localizeCommandError(notificationsError, t)}
            </p>
          ) : syncNotifications.length === 0 ? (
            <p className="sync-notification-empty">
              {t("syncNotifications.empty")}
            </p>
          ) : (
            syncNotifications.map((notification) => (
              <article
                key={notification.id}
                className={`sync-notification-item ${notification.read_at ? "read" : "unread"}`}
              >
                <div className="sync-notification-item-main">
                  <div className="sync-notification-item-meta">
                    <span>{t(categoryKey(notification.category))}</span>
                    <span>{t(sourceKey(notification.source))}</span>
                    <time dateTime={notification.occurred_at}>
                      {formatDate(notification.occurred_at)}
                    </time>
                  </div>
                  <p>{summary(notification, t)}</p>
                  {notification.protocol_version !== null && (
                    <small>
                      {t("syncNotifications.protocol", {
                        protocol: notification.protocol_version,
                        generation: notification.manifest_generation ?? 0,
                      })}
                    </small>
                  )}
                </div>
                <div className="sync-notification-item-actions">
                  {notification.retryable && (
                    <button type="button" className="about-link" onClick={() => void onSync()}>
                      {t("syncNotifications.syncNow")}
                    </button>
                  )}
                  {notification.read_at === null && (
                    <button
                      type="button"
                      className="about-link"
                      onClick={() => void markSyncNotificationRead(notification.id).catch(() => {})}
                    >
                      {t("syncNotifications.markRead")}
                    </button>
                  )}
                  <button
                    type="button"
                    className="about-link"
                    onClick={() => void dismissSyncNotification(notification.id).catch(() => {})}
                  >
                    {t("syncNotifications.dismiss")}
                  </button>
                </div>
              </article>
            ))
          )}
        </div>
      </ModalSurface>
    </div>
  );
}
