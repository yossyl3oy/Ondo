import { useRef, useEffect } from "react";
import "./NetworkGraph.css";

const MAX_POINTS = 30;

interface NetworkGraphProps {
  receivedPerSec: number;
  sentPerSec: number;
  formatSpeed: (bytes: number) => string;
}

interface SpeedHistory {
  dl: number[];
  ul: number[];
}

export function NetworkGraph({ receivedPerSec, sentPerSec, formatSpeed }: NetworkGraphProps) {
  const historyRef = useRef<SpeedHistory>({ dl: [], ul: [] });

  // Update history on each render (called every poll interval)
  useEffect(() => {
    const h = historyRef.current;
    h.dl.push(receivedPerSec);
    h.ul.push(sentPerSec);
    if (h.dl.length > MAX_POINTS) h.dl.shift();
    if (h.ul.length > MAX_POINTS) h.ul.shift();
  }, [receivedPerSec, sentPerSec]);

  const h = historyRef.current;
  const allValues = [...h.dl, ...h.ul];
  const maxVal = Math.max(...allValues, 1024); // min 1KB/s scale

  const width = 200;
  const height = 40;
  const padY = 2;
  const graphH = height - padY * 2;

  const toPath = (data: number[]): string => {
    if (data.length < 2) return "";
    const step = width / (MAX_POINTS - 1);
    const offset = (MAX_POINTS - data.length) * step;
    return data
      .map((v, i) => {
        const x = offset + i * step;
        const y = padY + graphH - (v / maxVal) * graphH;
        return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
      })
      .join(" ");
  };

  const toArea = (data: number[]): string => {
    const path = toPath(data);
    if (!path) return "";
    const step = width / (MAX_POINTS - 1);
    const offset = (MAX_POINTS - data.length) * step;
    const startX = offset;
    const endX = offset + (data.length - 1) * step;
    return `${path} L${endX.toFixed(1)},${height} L${startX.toFixed(1)},${height} Z`;
  };

  return (
    <div className="network-graph">
      <div className="network-graph-header">
        <span className="network-graph-scale">{formatSpeed(maxVal)}</span>
      </div>
      <svg
        className="network-graph-svg"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
      >
        {/* Grid lines */}
        <line x1="0" y1={padY + graphH * 0.5} x2={width} y2={padY + graphH * 0.5} className="network-graph-grid" />

        {/* DL area + line */}
        <path d={toArea(h.dl)} className="network-graph-area dl" />
        <path d={toPath(h.dl)} className="network-graph-line dl" />

        {/* UL area + line */}
        <path d={toArea(h.ul)} className="network-graph-area ul" />
        <path d={toPath(h.ul)} className="network-graph-line ul" />
      </svg>
    </div>
  );
}
