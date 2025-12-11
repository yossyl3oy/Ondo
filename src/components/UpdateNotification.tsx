import type { UpdateInfo } from "../hooks/useUpdater";
import "./UpdateNotification.css";

interface UpdateNotificationProps {
  updateInfo: UpdateInfo;
  downloading: boolean;
  progress: number;
  error: string | null;
  onUpdate: () => void;
  onDismiss: () => void;
}

export function UpdateNotification({
  updateInfo,
  downloading,
  progress,
  error,
  onUpdate,
  onDismiss,
}: UpdateNotificationProps) {
  if (!updateInfo.available) return null;

  return (
    <div className="update-notification">
      <div className="update-header">
        <span className="update-icon">↑</span>
        <span className="update-title">UPDATE</span>
        <button className="update-dismiss" onClick={onDismiss}>
          ×
        </button>
      </div>

      <div className="update-content">
        {error ? (
          <span className="update-error">{error}</span>
        ) : downloading ? (
          <>
            <span className="update-status">Downloading...</span>
            <div className="update-progress">
              <div
                className="update-progress-fill"
                style={{ width: `${progress}%` }}
              />
            </div>
            <span className="update-percent">{progress}%</span>
          </>
        ) : (
          <>
            <span className="update-version">
              v{updateInfo.version} available
            </span>
            <button className="update-btn" onClick={onUpdate}>
              INSTALL
            </button>
          </>
        )}
      </div>
    </div>
  );
}
