import { useState, useCallback } from 'react';
import type { CSSProperties } from 'react';
import type { KeyDef } from './types';
import { KEY_STYLE_COLORS, SECONDARY_LABEL_COLORS, isNumberKey } from './types';
import { darkenColor, lightenColor } from './utils';

interface KeyButtonProps {
  keyDef: KeyDef;
  onDown: () => void;
  onUp: () => void;
  width?: string | number;
  height?: string | number;
}

const LABEL_HEIGHT = 12;
const LABEL_FONT_SIZE = 9;

export function KeyButton({ keyDef, onDown, onUp, width, height }: KeyButtonProps) {
  const [isPressed, setIsPressed] = useState(false);

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setIsPressed(true);
    onDown();
  }, [onDown]);

  const handlePointerUp = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    setIsPressed(false);
    onUp();
  }, [onUp]);

  const handlePointerCancel = useCallback((e: React.PointerEvent) => {
    (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    setIsPressed(false);
    onUp();
  }, [onUp]);

  const styleColors = KEY_STYLE_COLORS[keyDef.style];
  const baseColor = styleColors.background;
  const borderColor = darkenColor(baseColor, styleColors.borderDarken);

  const topColor = isPressed
    ? darkenColor(baseColor, 0.22)
    : lightenColor(baseColor, 0.16);
  const bottomColor = isPressed
    ? darkenColor(baseColor, 0.32)
    : darkenColor(baseColor, 0.18);

  const containerStyle: CSSProperties = {
    display: 'flex',
    flexDirection: 'column',
    width: width ?? '100%',
    height: height ?? '100%',
    gap: 0,
  };

  const labelRowStyle: CSSProperties = {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    height: LABEL_HEIGHT,
    paddingLeft: 3,
    paddingRight: 3,
    fontSize: LABEL_FONT_SIZE,
    fontWeight: 600,
  };

  const buttonStyle: CSSProperties = {
    flex: 1,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    background: `linear-gradient(to bottom, ${topColor}, ${bottomColor})`,
    border: `${styleColors.borderWidth}px solid ${borderColor}`,
    borderRadius: styleColors.cornerRadius,
    cursor: 'pointer',
    touchAction: 'none',
    userSelect: 'none',
    WebkitUserSelect: 'none',
    transition: 'transform 0.05s',
    transform: isPressed ? 'scale(0.97)' : 'scale(1)',
  };

  const labelStyle: CSSProperties = {
    color: styleColors.text,
    fontSize: isNumberKey(keyDef.label) ? 22 : 13,
    fontWeight: keyDef.style === 'white' || keyDef.style === 'blue' ? 700 : 600,
    lineHeight: 1,
    whiteSpace: 'nowrap',
  };

  return (
    <div style={containerStyle}>
      {/* Secondary labels row */}
      <div style={labelRowStyle}>
        <span style={{
          color: keyDef.secondLabelColor ?? SECONDARY_LABEL_COLORS.second,
          opacity: keyDef.secondLabel ? 1 : 0,
        }}>
          {keyDef.secondLabel ?? ' '}
        </span>
        <span style={{
          color: keyDef.alphaLabelColor ?? SECONDARY_LABEL_COLORS.alpha,
          opacity: keyDef.alphaLabel ? 1 : 0,
        }}>
          {keyDef.alphaLabel ?? ' '}
        </span>
      </div>

      {/* Main button */}
      <div
        style={buttonStyle}
        onPointerDown={handlePointerDown}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerCancel}
        onPointerLeave={(e) => {
          if (isPressed) {
            handlePointerCancel(e);
          }
        }}
      >
        <span style={labelStyle}>{keyDef.label}</span>
      </div>
    </div>
  );
}
