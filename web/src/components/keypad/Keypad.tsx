import { useCallback } from "react";
import type { CSSProperties } from "react";
import { ImageButton } from "./ImageButton";
import { DPad } from "./DPad";
import { BUTTON_REGIONS, DPAD_REGION, KEYPAD_AREA } from "./buttonRegions";

interface KeypadProps {
  onKeyDown: (row: number, col: number) => void;
  onKeyUp: (row: number, col: number) => void;
}

export function Keypad({ onKeyDown, onKeyUp }: KeypadProps) {
  const makeOnDown = useCallback(
    (row: number, col: number) => () => {
      onKeyDown(row, col);
    },
    [onKeyDown],
  );

  const makeOnUp = useCallback(
    (row: number, col: number) => () => {
      onKeyUp(row, col);
    },
    [onKeyUp],
  );

  // Positioned over the keypad portion of the combined calculator_body image
  const containerStyle: CSSProperties = {
    position: "absolute",
    left: 0,
    top: `${KEYPAD_AREA.top}%`,
    width: "100%",
    height: `${KEYPAD_AREA.height}%`,
    userSelect: "none",
    WebkitUserSelect: "none",
    overflow: "hidden",
  };

  return (
    <div style={containerStyle}>
      {/* Regular buttons */}
      {BUTTON_REGIONS.map((region) => (
        <ImageButton
          key={`${region.row}-${region.col}`}
          region={region}
          onDown={makeOnDown(region.row, region.col)}
          onUp={makeOnUp(region.row, region.col)}
        />
      ))}

      {/* D-pad — expand 3px outward so the hit area covers the full circle */}
      <div
        style={{
          position: "absolute",
          left: `calc(${DPAD_REGION.left}% - 3px)`,
          top: `calc(${DPAD_REGION.top}% - 3px)`,
          width: `calc(${DPAD_REGION.width}% + 6px)`,
          height: `calc(${DPAD_REGION.height}% + 6px)`,
        }}
      >
        <DPad onKeyDown={onKeyDown} onKeyUp={onKeyUp} />
      </div>
    </div>
  );
}
