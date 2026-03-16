import type { SectionType } from "../types";
import "./RestorePanel.css";

const SECTION_LABELS: Record<SectionType, string> = {
  cpu: "CPU",
  gpu: "GPU",
  storage: "Storage",
  motherboard: "Motherboard",
  audio: "Audio",
};

interface RestorePanelProps {
  hiddenSections: SectionType[];
  onRestore: (type: SectionType) => void;
  onClose: () => void;
}

export function RestorePanel({ hiddenSections, onRestore, onClose }: RestorePanelProps) {
  return (
    <div className="settings-overlay" onClick={onClose}>
      <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <span className="settings-title">HIDDEN SECTIONS</span>
          <button className="settings-close" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="settings-content">
          {hiddenSections.map((type) => (
            <div key={type} className="restore-item">
              <div className="restore-item-info">
                <div className={`section-indicator ${type}`} />
                <span className="restore-item-label">{SECTION_LABELS[type]}</span>
              </div>
              <button
                className="restore-item-btn"
                onClick={() => onRestore(type)}
              >
                Restore
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
