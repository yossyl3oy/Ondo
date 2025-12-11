import { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import "./BootSequence.css";

interface BootLine {
  text: string;
  delay: number;
  type: "system" | "status" | "highlight";
}

const createBootLines = (version: string): BootLine[] => [
  { text: `ONDO SYSTEM v${version}`, delay: 0, type: "highlight" },
  { text: "Initializing hardware interface...", delay: 200, type: "system" },
  { text: "Connecting to sensor modules...", delay: 500, type: "system" },
  { text: "CPU sensor: ONLINE", delay: 800, type: "status" },
  { text: "GPU sensor: ONLINE", delay: 1000, type: "status" },
  { text: "System ready.", delay: 1400, type: "highlight" },
];

export function BootSequence() {
  const [visibleLines, setVisibleLines] = useState<number>(0);
  const [scanlineActive, setScanlineActive] = useState(true);
  const [bootLines, setBootLines] = useState<BootLine[]>(createBootLines("1.1.8"));

  useEffect(() => {
    getVersion().then((v) => {
      setBootLines(createBootLines(v));
    }).catch(() => {});
  }, []);

  useEffect(() => {
    bootLines.forEach((line, index) => {
      setTimeout(() => {
        setVisibleLines(index + 1);
      }, line.delay);
    });

    // Fade out scanline effect
    setTimeout(() => {
      setScanlineActive(false);
    }, 2500);
  }, [bootLines]);

  return (
    <div className="boot-sequence">
      {/* Animated scan line */}
      {scanlineActive && <div className="boot-scanline" />}

      {/* Hexagonal frame decoration */}
      <div className="boot-frame">
        <div className="hex-corner top-left" />
        <div className="hex-corner top-right" />
        <div className="hex-corner bottom-left" />
        <div className="hex-corner bottom-right" />
      </div>

      {/* Boot terminal */}
      <div className="boot-terminal">
        <div className="boot-header">
          <div className="boot-header-line" />
          <span className="boot-header-text">SYSTEM BOOT</span>
          <div className="boot-header-line" />
        </div>

        <div className="boot-content">
          {bootLines.slice(0, visibleLines).map((line, index) => (
            <div
              key={index}
              className={`boot-line ${line.type}`}
              style={{ animationDelay: `${index * 0.1}s` }}
            >
              <span className="boot-prefix">{">"}</span>
              <span className="boot-text">{line.text}</span>
              {index === visibleLines - 1 && (
                <span className="boot-cursor">_</span>
              )}
            </div>
          ))}
        </div>

        {/* Loading bar */}
        <div className="boot-progress">
          <div
            className="boot-progress-bar"
            style={{ width: `${(visibleLines / bootLines.length) * 100}%` }}
          />
        </div>
      </div>

      {/* Decorative elements */}
      <div className="boot-decoration left">
        <div className="deco-bar" />
        <div className="deco-bar short" />
        <div className="deco-bar" />
      </div>
      <div className="boot-decoration right">
        <div className="deco-bar" />
        <div className="deco-bar short" />
        <div className="deco-bar" />
      </div>
    </div>
  );
}
