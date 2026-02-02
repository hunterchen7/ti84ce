import { useState, useCallback, useRef, useEffect } from 'react';
import type { CSSProperties } from 'react';
import { darkenColor, lightenColor } from './utils';

type DPadDirection = 'up' | 'down' | 'left' | 'right';

interface DPadProps {
  onKeyDown: (row: number, col: number) => void;
  onKeyUp: (row: number, col: number) => void;
  size?: number;
}

// D-pad key coordinates (row, col)
const DIRECTION_COORDS: Record<DPadDirection, [number, number]> = {
  up: [7, 3],
  down: [7, 0],
  left: [7, 1],
  right: [7, 2],
};

// Angles for each direction (in degrees)
const DIRECTION_ANGLES: Record<DPadDirection, number> = {
  up: 270,
  down: 90,
  left: 180,
  right: 0,
};

// Colors
const SEGMENT_COLOR = '#E3E3E3';
const PRESSED_COLOR = '#CECECE';
const BORDER_COLOR = '#B5B5B5';
const ARROW_COLOR = '#2B2B2B';
const GAP_COLOR = '#1B1B1B';
const CENTER_COLOR = '#1B1B1B';
const CENTER_DOT_COLOR = '#595959';

// Configuration
const SWEEP_ANGLE = 90;
const INNER_RADIUS_SCALE = 0.45;
const GAP_WIDTH_SCALE = 0.16;

export function DPad({ onKeyDown, onKeyUp, size = 120 }: DPadProps) {
  const [pressedDirection, setPressedDirection] = useState<DPadDirection | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Hit test for D-pad segments
  const hitTest = useCallback((x: number, y: number): DPadDirection | null => {
    const center = size / 2;
    const dx = x - center;
    const dy = y - center;
    const radius = Math.sqrt(dx * dx + dy * dy);

    const outerRadius = size / 2;
    const innerRadius = outerRadius * INNER_RADIUS_SCALE;

    // Check radius bounds
    if (radius < innerRadius || radius > outerRadius) {
      return null;
    }

    // Check if in gap (diagonal lines)
    const gapWidth = outerRadius * GAP_WIDTH_SCALE;
    const threshold = gapWidth * 0.5 * 1.41421356;
    if (Math.abs(dy - dx) < threshold || Math.abs(dy + dx) < threshold) {
      return null;
    }

    // Calculate angle
    let angle = Math.atan2(dy, dx) * 180 / Math.PI;
    if (angle < 0) angle += 360;

    // Check which segment
    const directions: DPadDirection[] = ['up', 'down', 'left', 'right'];
    for (const dir of directions) {
      const dirAngle = DIRECTION_ANGLES[dir];
      const startAngle = (dirAngle - SWEEP_ANGLE / 2 + 360) % 360;
      let endAngle = (dirAngle + SWEEP_ANGLE / 2) % 360;

      if (startAngle <= endAngle) {
        if (angle >= startAngle && angle <= endAngle) return dir;
      } else {
        if (angle >= startAngle || angle <= endAngle) return dir;
      }
    }

    return null;
  }, [size]);

  // Draw the D-pad
  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Clear
    ctx.clearRect(0, 0, size, size);

    const center = size / 2;
    const outerRadius = size / 2 - 2; // Small margin
    const innerRadius = outerRadius * INNER_RADIUS_SCALE;
    const strokeWidth = outerRadius * 0.035;

    // Draw segments
    const directions: DPadDirection[] = ['up', 'down', 'left', 'right'];
    for (const dir of directions) {
      const isPressed = pressedDirection === dir;
      const fillColor = isPressed ? PRESSED_COLOR : SEGMENT_COLOR;
      const dirAngle = DIRECTION_ANGLES[dir];
      const startAngle = (dirAngle - SWEEP_ANGLE / 2) * Math.PI / 180;
      const endAngle = (dirAngle + SWEEP_ANGLE / 2) * Math.PI / 180;

      // Create segment path
      ctx.beginPath();
      ctx.arc(center, center, outerRadius - strokeWidth * 0.2, startAngle, endAngle);
      ctx.arc(center, center, innerRadius + strokeWidth * 0.15, endAngle, startAngle, true);
      ctx.closePath();

      // Fill
      ctx.fillStyle = fillColor;
      ctx.fill();

      // Border
      const rimColor = isPressed
        ? darkenColor(BORDER_COLOR, 0.35)
        : lightenColor(BORDER_COLOR, 0.35);
      ctx.strokeStyle = rimColor;
      ctx.lineWidth = strokeWidth;
      ctx.stroke();

      // Draw arrow
      const arrowRadius = (innerRadius + outerRadius) * 0.5;
      const angleRad = dirAngle * Math.PI / 180;
      const arrowCenterX = center + Math.cos(angleRad) * arrowRadius;
      const arrowCenterY = center + Math.sin(angleRad) * arrowRadius;

      const arrowLength = outerRadius * 0.09;
      const arrowWidth = outerRadius * 0.16;

      const forward = { x: Math.cos(angleRad), y: Math.sin(angleRad) };
      const perpendicular = { x: -forward.y, y: forward.x };

      const tip = {
        x: arrowCenterX + forward.x * arrowLength,
        y: arrowCenterY + forward.y * arrowLength,
      };
      const baseCenter = {
        x: arrowCenterX - forward.x * (arrowLength * 0.45),
        y: arrowCenterY - forward.y * (arrowLength * 0.45),
      };
      const left = {
        x: baseCenter.x + perpendicular.x * (arrowWidth * 0.5),
        y: baseCenter.y + perpendicular.y * (arrowWidth * 0.5),
      };
      const right = {
        x: baseCenter.x - perpendicular.x * (arrowWidth * 0.5),
        y: baseCenter.y - perpendicular.y * (arrowWidth * 0.5),
      };

      ctx.beginPath();
      ctx.moveTo(tip.x, tip.y);
      ctx.lineTo(left.x, left.y);
      ctx.lineTo(right.x, right.y);
      ctx.closePath();
      ctx.fillStyle = ARROW_COLOR;
      ctx.fill();
    }

    // Draw gaps
    const gapWidth = outerRadius * GAP_WIDTH_SCALE;
    const rectLength = outerRadius * 2.1;

    ctx.fillStyle = GAP_COLOR;

    // 45 degree gap
    ctx.save();
    ctx.translate(center, center);
    ctx.rotate(45 * Math.PI / 180);
    ctx.fillRect(-gapWidth / 2, -rectLength / 2, gapWidth, rectLength);
    ctx.restore();

    // -45 degree gap
    ctx.save();
    ctx.translate(center, center);
    ctx.rotate(-45 * Math.PI / 180);
    ctx.fillRect(-gapWidth / 2, -rectLength / 2, gapWidth, rectLength);
    ctx.restore();

    // Center circle
    ctx.beginPath();
    ctx.arc(center, center, size * 0.125, 0, Math.PI * 2);
    ctx.fillStyle = CENTER_COLOR;
    ctx.fill();
  }, [size, pressedDirection]);

  // Redraw when pressed state changes
  useEffect(() => {
    draw();
  }, [draw]);

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);

    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;

    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const hit = hitTest(x, y);

    if (hit) {
      setPressedDirection(hit);
      const coords = DIRECTION_COORDS[hit];
      onKeyDown(coords[0], coords[1]);
    }
  }, [hitTest, onKeyDown]);

  const handlePointerMove = useCallback((e: React.PointerEvent) => {
    if (pressedDirection === null) return;

    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;

    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const hit = hitTest(x, y);

    if (hit !== pressedDirection) {
      // Release previous
      if (pressedDirection) {
        const coords = DIRECTION_COORDS[pressedDirection];
        onKeyUp(coords[0], coords[1]);
      }
      // Press new (if any)
      if (hit) {
        setPressedDirection(hit);
        const coords = DIRECTION_COORDS[hit];
        onKeyDown(coords[0], coords[1]);
      } else {
        setPressedDirection(null);
      }
    }
  }, [pressedDirection, hitTest, onKeyDown, onKeyUp]);

  const handlePointerUp = useCallback((e: React.PointerEvent) => {
    (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    if (pressedDirection) {
      const coords = DIRECTION_COORDS[pressedDirection];
      onKeyUp(coords[0], coords[1]);
      setPressedDirection(null);
    }
  }, [pressedDirection, onKeyUp]);

  const containerStyle: CSSProperties = {
    width: size,
    height: size,
    touchAction: 'none',
    userSelect: 'none',
    WebkitUserSelect: 'none',
    cursor: 'pointer',
  };

  return (
    <div
      ref={containerRef}
      style={containerStyle}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
      onPointerLeave={(e) => {
        if (pressedDirection) {
          handlePointerUp(e);
        }
      }}
    >
      <canvas ref={canvasRef} width={size} height={size} />
    </div>
  );
}
