// Key definition types for TI-84 Plus CE keypad

/** Visual style for calculator keys */
export type KeyStyle = "dark" | "yellow" | "green" | "white" | "blue" | "arrow";

/** Colors for each key style */
export const KEY_STYLE_COLORS: Record<
  KeyStyle,
  {
    background: string;
    text: string;
    borderDarken: number;
    cornerRadius: number;
    borderWidth: number;
  }
> = {
  yellow: {
    background: "#6AB6E6",
    text: "#1A1A1A",
    borderDarken: 0.35,
    cornerRadius: 7,
    borderWidth: 1,
  },
  green: {
    background: "#6DBE45",
    text: "#1A1A1A",
    borderDarken: 0.35,
    cornerRadius: 7,
    borderWidth: 1,
  },
  white: {
    background: "#E6E6E6",
    text: "#1A1A1A",
    borderDarken: 0.4,
    cornerRadius: 4,
    borderWidth: 1.5,
  },
  blue: {
    background: "#DCDCDC",
    text: "#1A1A1A",
    borderDarken: 0.4,
    cornerRadius: 4,
    borderWidth: 1.5,
  },
  arrow: {
    background: "#4A4A4A",
    text: "#F7F7F7",
    borderDarken: 0.48,
    cornerRadius: 6,
    borderWidth: 1,
  },
  dark: {
    background: "#2D2D2D",
    text: "#F7F7F7",
    borderDarken: 0.48,
    cornerRadius: 6,
    borderWidth: 1,
  },
};

/** Default colors for secondary labels */
export const SECONDARY_LABEL_COLORS = {
  second: "#79C9FF", // Blue for 2nd function
  alpha: "#7EC64B", // Green for alpha
};

/** Definition of a single calculator key */
export interface KeyDef {
  label: string;
  row: number;
  col: number;
  style: KeyStyle;
  secondLabel?: string;
  alphaLabel?: string;
  secondLabelColor?: string;
  alphaLabelColor?: string;
}

/** Create a key definition with defaults */
export function createKeyDef(
  label: string,
  row: number,
  col: number,
  style: KeyStyle = "dark",
  secondLabel?: string,
  alphaLabel?: string,
): KeyDef {
  return {
    label,
    row,
    col,
    style,
    secondLabel,
    alphaLabel,
  };
}

/** Check if key is a number key (larger font) */
export function isNumberKey(label: string): boolean {
  if (label.length === 1 && /\d/.test(label)) {
    return true;
  }
  return label === "." || label === "(-)";
}
