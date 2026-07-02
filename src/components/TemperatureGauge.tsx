import "./TemperatureGauge.css";

interface TemperatureGaugeProps {
  value: number;
  max: number;
  status: "normal" | "warning" | "danger";
  label: string;
  unit?: string;
}

// Arc geometry for the gauge
const RADIUS = 28;
const STROKE_WIDTH = 4;
const CENTER = 32;
const START_ANGLE = 135;
const END_ANGLE = 405;
const TOTAL_ANGLE = END_ANGLE - START_ANGLE;

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

// The background arc and tick marks never change — compute them once.
const BACKGROUND_ARC = describeArc(CENTER, CENTER, RADIUS, START_ANGLE, END_ANGLE);
const TICK_LINES = [0, 25, 50, 75, 100].map((tick) => {
  const tickAngle = START_ANGLE + (tick / 100) * TOTAL_ANGLE;
  const innerPoint = polarToCartesian(CENTER, CENTER, RADIUS - 6, tickAngle);
  const outerPoint = polarToCartesian(CENTER, CENTER, RADIUS - 2, tickAngle);
  return { tick, innerPoint, outerPoint };
});

export function TemperatureGauge({
  value,
  max,
  status,
  label,
  unit = "℃",
}: TemperatureGaugeProps) {
  const isAvailable = value > 0;
  const percentage = isAvailable ? Math.min((value / max) * 100, 100) : 0;
  const currentAngle = START_ANGLE + (percentage / 100) * TOTAL_ANGLE;
  const valueArc = describeArc(CENTER, CENTER, RADIUS, START_ANGLE, currentAngle);

  return (
    <div className={`temp-gauge ${status}`}>
      <svg viewBox="0 0 64 64" className="gauge-svg">
        {/* Background arc */}
        <path
          d={BACKGROUND_ARC}
          fill="none"
          stroke="var(--hud-border)"
          strokeWidth={STROKE_WIDTH}
          strokeLinecap="round"
        />
        {/* Value arc */}
        <path
          d={valueArc}
          fill="none"
          className="gauge-value-arc"
          strokeWidth={STROKE_WIDTH}
          strokeLinecap="round"
        />
        {/* Tick marks */}
        {TICK_LINES.map(({ tick, innerPoint, outerPoint }) => (
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
        ))}
      </svg>
      <div className="gauge-content">
        <span className={`gauge-value ${!isAvailable ? 'unavailable' : ''}`}>
          {isAvailable ? <>{Math.round(value)}<span className="gauge-unit">{unit}</span></> : 'N/A'}
        </span>
        <span className="gauge-label">{label}</span>
      </div>
    </div>
  );
}
