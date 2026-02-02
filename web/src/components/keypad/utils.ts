// Utility functions for keypad components

/** Parse hex color to RGB */
export function hexToRgb(hex: string): { r: number; g: number; b: number } {
  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  if (!result) {
    return { r: 0, g: 0, b: 0 };
  }
  return {
    r: parseInt(result[1], 16),
    g: parseInt(result[2], 16),
    b: parseInt(result[3], 16),
  };
}

/** Convert RGB to hex color */
export function rgbToHex(r: number, g: number, b: number): string {
  const clamp = (v: number) => Math.max(0, Math.min(255, Math.round(v)));
  return `#${clamp(r).toString(16).padStart(2, '0')}${clamp(g).toString(16).padStart(2, '0')}${clamp(b).toString(16).padStart(2, '0')}`;
}

/** Blend two colors together */
export function blendColors(color1: string, color2: string, ratio: number): string {
  const c1 = hexToRgb(color1);
  const c2 = hexToRgb(color2);
  const clamped = Math.max(0, Math.min(1, ratio));

  return rgbToHex(
    c1.r + (c2.r - c1.r) * clamped,
    c1.g + (c2.g - c1.g) * clamped,
    c1.b + (c2.b - c1.b) * clamped
  );
}

/** Darken a color by blending with black */
export function darkenColor(color: string, amount: number): string {
  return blendColors(color, '#000000', amount);
}

/** Lighten a color by blending with white */
export function lightenColor(color: string, amount: number): string {
  return blendColors(color, '#FFFFFF', amount);
}
