// Auto-generated button regions from hires-ti84ce-cropped.png extraction
// Coordinates are percentages relative to the keypad body image

export const KEYPAD_ASPECT_RATIO = 0.657338; // width / height

// Combined calculator body (bezel + keypad in one image)
export const BODY_ASPECT_RATIO = 963 / 2239; // width / height

// LCD canvas position as percentages of the combined body image
export const LCD_POSITION = {
  left: 11.53,
  top: 6.92,
  width: 76.74,
  height: 24.92,
};

// Keypad area within the combined body image (percentages)
export const KEYPAD_AREA = {
  top: 34.57,
  height: 65.43,
};

export interface ButtonRegion {
  name: string;
  row: number;
  col: number;
  left: number;   // percentage
  top: number;    // percentage
  width: number;  // percentage
  height: number; // percentage
  img: string;    // image filename
}

export const BUTTON_REGIONS: ButtonRegion[] = [
  // Function row
  { name: "y=",       row: 1, col: 4, left: 11.32, top:  5.12, width: 11.01, height: 4.03, img: "btn_y_eq.png" },
  { name: "window",   row: 1, col: 3, left: 27.73, top:  5.12, width: 11.32, height: 4.03, img: "btn_window.png" },
  { name: "zoom",     row: 1, col: 2, left: 44.76, top:  5.12, width: 10.80, height: 4.03, img: "btn_zoom.png" },
  { name: "trace",    row: 1, col: 1, left: 61.37, top:  5.12, width: 10.70, height: 4.03, img: "btn_trace.png" },
  { name: "graph",    row: 1, col: 0, left: 77.78, top:  5.12, width: 11.01, height: 4.03, img: "btn_graph.png" },

  // Control row 1
  { name: "2nd",      row: 1, col: 5, left: 11.32, top: 14.20, width: 11.01, height: 5.32, img: "btn_2nd.png" },
  { name: "mode",     row: 1, col: 6, left: 27.73, top: 14.20, width: 11.01, height: 5.32, img: "btn_mode.png" },
  { name: "del",      row: 1, col: 7, left: 44.65, top: 14.20, width: 11.01, height: 5.32, img: "btn_del.png" },

  // Control row 2
  { name: "alpha",    row: 2, col: 7, left: 11.32, top: 22.73, width: 11.01, height: 5.32, img: "btn_alpha.png" },
  { name: "X,T,θ,n",  row: 3, col: 7, left: 27.73, top: 22.53, width: 11.01, height: 5.53, img: "btn_xttn.png" },
  { name: "stat",     row: 4, col: 7, left: 44.65, top: 22.73, width: 11.01, height: 5.32, img: "btn_stat.png" },

  // Math row
  { name: "math",     row: 2, col: 6, left: 11.11, top: 31.13, width: 11.11, height: 5.46, img: "btn_math.png" },
  { name: "apps",     row: 3, col: 6, left: 28.04, top: 31.13, width: 10.70, height: 5.46, img: "btn_apps.png" },
  { name: "prgm",     row: 4, col: 6, left: 44.65, top: 31.19, width: 11.01, height: 5.32, img: "btn_prgm.png" },
  { name: "vars",     row: 5, col: 6, left: 61.37, top: 31.19, width: 10.70, height: 5.32, img: "btn_vars.png" },
  { name: "clear",    row: 6, col: 6, left: 77.47, top: 31.19, width: 11.32, height: 5.46, img: "btn_clear.png" },

  // Trig row
  { name: "x_inv",    row: 2, col: 5, left: 11.01, top: 39.66, width: 11.32, height: 5.67, img: "btn_x_inv.png" },
  { name: "sin",      row: 3, col: 5, left: 28.04, top: 39.66, width: 10.70, height: 5.32, img: "btn_sin.png" },
  { name: "cos",      row: 4, col: 5, left: 44.65, top: 39.66, width: 11.01, height: 5.32, img: "btn_cos.png" },
  { name: "tan",      row: 5, col: 5, left: 61.37, top: 39.66, width: 10.70, height: 5.32, img: "btn_tan.png" },
  { name: "pow",      row: 6, col: 5, left: 77.78, top: 39.66, width: 11.01, height: 5.46, img: "btn_pow.png" },

  // Special row
  { name: "x_sq",     row: 2, col: 4, left: 11.32, top: 48.19, width: 11.01, height: 5.32, img: "btn_x_sq.png" },
  { name: "comma",    row: 3, col: 4, left: 28.04, top: 48.19, width: 10.70, height: 5.32, img: "btn_comma.png" },
  { name: "lparen",   row: 4, col: 4, left: 44.65, top: 48.40, width: 11.01, height: 5.12, img: "btn_lparen.png" },
  { name: "rparen",   row: 5, col: 4, left: 61.37, top: 48.40, width: 10.70, height: 5.12, img: "btn_rparen.png" },
  { name: "div",      row: 6, col: 4, left: 77.67, top: 48.19, width: 11.11, height: 5.32, img: "btn_div.png" },

  // Number block row 1
  { name: "log",      row: 2, col: 3, left: 11.32, top: 56.93, width: 11.01, height: 5.12, img: "btn_log.png" },
  { name: "7",        row: 3, col: 3, left: 27.93, top: 56.79, width: 11.01, height: 6.21, img: "btn_7.png" },
  { name: "8",        row: 4, col: 3, left: 44.65, top: 56.66, width: 11.01, height: 6.21, img: "btn_8.png" },
  { name: "9",        row: 5, col: 3, left: 61.16, top: 56.93, width: 11.01, height: 6.21, img: "btn_9.png" },
  { name: "mul",      row: 6, col: 3, left: 77.78, top: 56.79, width: 11.01, height: 5.19, img: "btn_mul.png" },

  // Number block row 2
  { name: "ln",       row: 2, col: 2, left: 11.32, top: 65.12, width: 11.11, height: 5.53, img: "btn_ln.png" },
  { name: "4",        row: 3, col: 2, left: 27.93, top: 66.35, width: 11.01, height: 6.21, img: "btn_4.png" },
  { name: "5",        row: 4, col: 2, left: 44.65, top: 66.35, width: 11.01, height: 6.21, img: "btn_5.png" },
  { name: "6",        row: 5, col: 2, left: 61.16, top: 66.35, width: 11.01, height: 6.21, img: "btn_6.png" },
  { name: "sub",      row: 6, col: 2, left: 77.78, top: 65.39, width: 11.01, height: 5.12, img: "btn_sub.png" },

  // Number block row 3
  { name: "sto",      row: 2, col: 1, left: 11.01, top: 73.52, width: 11.63, height: 5.67, img: "btn_sto.png" },
  { name: "1",        row: 3, col: 1, left: 27.93, top: 76.04, width: 11.01, height: 6.21, img: "btn_1.png" },
  { name: "2",        row: 4, col: 1, left: 44.65, top: 76.04, width: 11.01, height: 6.21, img: "btn_2.png" },
  { name: "3",        row: 5, col: 1, left: 61.16, top: 75.84, width: 11.01, height: 6.21, img: "btn_3.png" },
  { name: "add",      row: 6, col: 1, left: 77.78, top: 73.58, width: 11.01, height: 5.32, img: "btn_add.png" },

  // Number block row 4
  { name: "on",       row: 2, col: 0, left: 11.11, top: 81.91, width: 11.11, height: 5.94, img: "btn_on.png" },
  { name: "0",        row: 3, col: 0, left: 27.93, top: 85.80, width: 11.01, height: 6.21, img: "btn_0.png" },
  { name: "dot",      row: 4, col: 0, left: 44.65, top: 85.80, width: 11.01, height: 6.21, img: "btn_dot.png" },
  { name: "neg",      row: 5, col: 0, left: 61.16, top: 85.80, width: 11.01, height: 6.21, img: "btn_neg.png" },
  { name: "enter",    row: 6, col: 0, left: 77.78, top: 82.39, width: 11.01, height: 5.12, img: "btn_enter.png" },
];

// D-pad region (handled separately for hit-testing)
export const DPAD_REGION = {
  row_up: 7, col_up: 3,
  row_down: 7, col_down: 0,
  row_left: 7, col_left: 1,
  row_right: 7, col_right: 2,
  left: 63.97,
  top: 13.72,
  width: 22.01,
  height: 14.74,
  img: "btn_dpad.png",
};
