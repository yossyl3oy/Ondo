import "./CpuCoreGrid.css";

interface CoreData {
  index: number;
  temperature: number;
  load: number;
}

interface CpuCoreGridProps {
  cores: CoreData[];
  maxTemp: number;
}

export function CpuCoreGrid({ cores, maxTemp }: CpuCoreGridProps) {
  const getTemperatureColor = (temp: number): string => {
    const ratio = temp / maxTemp;
    if (ratio >= 0.9) return "var(--hud-danger)";
    if (ratio >= 0.75) return "var(--hud-warning)";
    if (ratio >= 0.5) return "var(--hud-primary)";
    return "var(--hud-success)";
  };

  return (
    <div className="core-grid">
      <div className="core-grid-header">
        <span className="core-grid-title">CORE TEMPERATURES</span>
      </div>
      <div className="core-grid-content">
        {cores.map((core) => (
          <div key={core.index} className="core-item">
            <span className="core-index">C{core.index}</span>
            <div className="core-temp-bar">
              <div
                className="core-temp-fill"
                style={{
                  width: `${(core.temperature / maxTemp) * 100}%`,
                  backgroundColor: getTemperatureColor(core.temperature),
                }}
              />
            </div>
            <span
              className="core-temp-value"
              style={{ color: getTemperatureColor(core.temperature) }}
            >
              {Math.round(core.temperature)}°
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
