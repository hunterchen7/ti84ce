import { useState, useCallback } from "react";
import type { CSSProperties } from "react";
import type { ButtonRegion } from "./buttonRegions";

interface ImageButtonProps {
  region: ButtonRegion;
  onDown: () => void;
  onUp: () => void;
}

// Travel distance as percentage of button height
const TRAVEL_PX = 2;

export function ImageButton({ region, onDown, onUp }: ImageButtonProps) {
  const [isPressed, setIsPressed] = useState(false);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      setIsPressed(true);
      onDown();
    },
    [onDown],
  );

  const handlePointerUp = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
      setIsPressed(false);
      onUp();
    },
    [onUp],
  );

  const handlePointerCancel = useCallback(
    (e: React.PointerEvent) => {
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
      setIsPressed(false);
      onUp();
    },
    [onUp],
  );

  const containerStyle: CSSProperties = {
    position: "absolute",
    left: `${region.left}%`,
    top: `${region.top}%`,
    width: `${region.width}%`,
    height: `${region.height}%`,
    cursor: "pointer",
    touchAction: "none",
  };

  const imgStyle: CSSProperties = {
    width: "100%",
    height: "100%",
    display: "block",
    objectFit: "fill",
    // Travel effect: translate down when pressed, darken slightly
    transform: isPressed ? `translateY(${TRAVEL_PX}px)` : "translateY(0)",
    filter: isPressed ? "brightness(0.82)" : "brightness(1)",
    // Shadow for depth: elevated when unpressed, flush when pressed
    // Using drop-shadow on the image itself
    transition: isPressed
      ? "transform 50ms ease-out, filter 50ms ease-out"
      : "transform 120ms cubic-bezier(0.34, 1.56, 0.64, 1), filter 100ms ease-out",
    pointerEvents: "none",
  };

  return (
    <div
      style={containerStyle}
      onPointerDown={handlePointerDown}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerCancel}
      onPointerLeave={(e) => {
        if (isPressed) {
          handlePointerCancel(e);
        }
      }}
    >
      <img
        src={`/buttons/${region.img}`}
        alt={region.name}
        style={imgStyle}
        draggable={false}
      />
    </div>
  );
}
