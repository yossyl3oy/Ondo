import "./TemperatureGauge.css";

interface TemperatureGaugeProps {
  value: number;
  max: number;
  status: "normal" | "warning" | "danger";
  label: string;
}

export function TemperatureGauge({
  value,
  max,
  status,
  label,
}: TemperatureGaugeProps) {
  const percentage = Math.min((value / max) * 100, 100);

  // Arc path calculation for the gauge
  const radius = 28;
  const strokeWidth = 4;
  const center = 32;
  const startAngle = 135;
  const endAngle = 405;
  const totalAngle = endAngle - startAngle;
  const currentAngle = startAngle + (percentage / 100) * totalAngle;

  const polarToCartesian = (
    cx: number,
    cy: number,
    r: number,
    angle: number
  ) => {
    const rad = ((angle - 90) * Math.PI) / 180;
    return {
      x: cx + r * Math.cos(rad),
      y: cy + r * Math.sin(rad),
    };
  };

  const describeArc = (
    cx: number,
    cy: number,
    r: number,
    startAngle: number,
    endAngle: number
  ) => {
    const start = polarToCartesian(cx, cy, r, endAngle);
    const end = polarToCartesian(cx, cy, r, startAngle);
    const largeArcFlag = endAngle - startAngle <= 180 ? 0 : 1;
    return `M ${start.x} ${start.y} A ${r} ${r} 0 ${largeArcFlag} 0 ${end.x} ${end.y}`;
  };

  const backgroundArc = describeArc(center, center, radius, startAngle, endAngle);
  const valueArc = describeArc(center, center, radius, startAngle, currentAngle);

  return (
    <div className={`temp-gauge ${status}`}>
      <svg viewBox="0 0 64 64" className="gauge-svg">
        {/* Background arc */}
        <path
          d={backgroundArc}
          fill="none"
          stroke="var(--hud-border)"
          strokeWidth={strokeWidth}
          strokeLinecap="round"
        />
        {/* Value arc */}
        <path
          d={valueArc}
          fill="none"
          className="gauge-value-arc"
          strokeWidth={strokeWidth}
          strokeLinecap="round"
        />
        {/* Tick marks */}
        {[0, 25, 50, 75, 100].map((tick) => {
          const tickAngle = startAngle + (tick / 100) * totalAngle;
          const innerPoint = polarToCartesian(center, center, radius - 6, tickAngle);
          const outerPoint = polarToCartesian(center, center, radius - 2, tickAngle);
          return (
            <line
              key={tick}
              x1={innerPoint.x}
              y1={innerPoint.y}
              x2={outerPoint.x}
              y2={outerPoint.y}
              stroke="var(--hud-text-secondary)"
              strokeWidth="1"
              opacity="0.4"
            />
          );
        })}
      </svg>
      <div className="gauge-content">
        <span className="gauge-value">{value}°</span>
        <span className="gauge-label">{label}</span>
      </div>
    </div>
  );
}
