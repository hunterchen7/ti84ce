import { useCallback } from 'react';
import type { CSSProperties } from 'react';
import { KeyButton } from './KeyButton';
import { DPad } from './DPad';
import type { KeyStyle } from './types';
import { createKeyDef } from './types';

interface KeypadProps {
  onKeyDown: (row: number, col: number) => void;
  onKeyUp: (row: number, col: number) => void;
}

// Layout constants
const ROW_SPACING = 2;
const COLUMN_SPACING = 16;
const FUNC_ROW_HEIGHT = 36;
const CONTROL_ROW_HEIGHT = 42;
const NUMBER_BUTTON_HEIGHT = 56;
const SIDE_BUTTON_HEIGHT = 48;

// Color
const BACKGROUND_COLOR = '#1B1B1B';

// Helper types
interface KeySpec {
  label: string;
  row: number;
  col: number;
  style?: KeyStyle;
  second?: string;
  alpha?: string;
}

function k(
  label: string,
  row: number,
  col: number,
  style?: KeyStyle,
  second?: string,
  alpha?: string
): KeySpec {
  return { label, row, col, style, second, alpha };
}

export function Keypad({ onKeyDown, onKeyUp }: KeypadProps) {
  const makeOnDown = useCallback((row: number, col: number) => () => {
    onKeyDown(row, col);
  }, [onKeyDown]);

  const makeOnUp = useCallback((row: number, col: number) => () => {
    onKeyUp(row, col);
  }, [onKeyUp]);

  const containerStyle: CSSProperties = {
    display: 'flex',
    flexDirection: 'column',
    gap: ROW_SPACING,
    padding: 6,
    background: BACKGROUND_COLOR,
    userSelect: 'none',
    WebkitUserSelect: 'none',
  };

  const rowStyle: CSSProperties = {
    display: 'flex',
    gap: COLUMN_SPACING,
  };

  // Render a key
  const renderKey = (spec: KeySpec, width: string | number, height: number) => (
    <div key={`${spec.row}-${spec.col}`} style={{ width, height }}>
      <KeyButton
        keyDef={createKeyDef(spec.label, spec.row, spec.col, spec.style ?? 'dark', spec.second, spec.alpha)}
        onDown={makeOnDown(spec.row, spec.col)}
        onUp={makeOnUp(spec.row, spec.col)}
      />
    </div>
  );

  // Render a flex column (equal width)
  const renderFlexKey = (spec: KeySpec, height: number) => (
    <div key={`${spec.row}-${spec.col}`} style={{ flex: 1, height }}>
      <KeyButton
        keyDef={createKeyDef(spec.label, spec.row, spec.col, spec.style ?? 'dark', spec.second, spec.alpha)}
        onDown={makeOnDown(spec.row, spec.col)}
        onUp={makeOnUp(spec.row, spec.col)}
      />
    </div>
  );

  // Row with 5 equal-width keys
  const renderFiveKeyRow = (keys: KeySpec[], height: number) => (
    <div style={{ ...rowStyle, height }}>
      {keys.map(key => renderFlexKey(key, height))}
    </div>
  );

  return (
    <div style={containerStyle}>
      {/* Row 1: Function keys (y=, window, zoom, trace, graph) */}
      {renderFiveKeyRow([
        k('y=', 1, 4, 'white', 'stat plot', 'f1'),
        k('window', 1, 3, 'white', 'tblset', 'f2'),
        k('zoom', 1, 2, 'white', 'format', 'f3'),
        k('trace', 1, 1, 'white', 'calc', 'f4'),
        k('graph', 1, 0, 'white', 'table', 'f5'),
      ], FUNC_ROW_HEIGHT)}

      {/* Rows 2-3: 2nd/mode/del + alpha/X,T,θ,n/stat with D-pad */}
      {/* Uses same 5-column equal-width layout - D-pad spans last 2 cols */}
      <div style={{ position: 'relative', height: CONTROL_ROW_HEIGHT * 2 + ROW_SPACING }}>
        {/* Full-width rows with spacers for D-pad area */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: ROW_SPACING }}>
          <div style={rowStyle}>
            {renderFlexKey(k('2nd', 1, 5, 'yellow'), CONTROL_ROW_HEIGHT)}
            {renderFlexKey(k('mode', 1, 6, 'dark', 'quit'), CONTROL_ROW_HEIGHT)}
            {renderFlexKey(k('del', 1, 7, 'dark', 'ins'), CONTROL_ROW_HEIGHT)}
            {/* Spacer for D-pad columns 4-5 */}
            <div style={{ flex: 1 }} />
            <div style={{ flex: 1 }} />
          </div>
          <div style={rowStyle}>
            {renderFlexKey(k('alpha', 2, 7, 'green', 'A-lock'), CONTROL_ROW_HEIGHT)}
            {renderFlexKey(k('X,T,θ,n', 3, 7, 'dark', 'link'), CONTROL_ROW_HEIGHT)}
            {renderFlexKey(k('stat', 4, 7, 'dark', 'list'), CONTROL_ROW_HEIGHT)}
            {/* Spacer for D-pad columns 4-5 */}
            <div style={{ flex: 1 }} />
            <div style={{ flex: 1 }} />
          </div>
        </div>
        {/* D-pad positioned in last 2 columns, shifted right */}
        <div style={{
          position: 'absolute',
          top: 0,
          right: -8,
          width: 'calc(40% + 16px)',
          height: '100%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
        }}>
          <DPad onKeyDown={onKeyDown} onKeyUp={onKeyUp} size={CONTROL_ROW_HEIGHT * 2} />
        </div>
      </div>

      {/* Row 4: math, apps, prgm, vars, clear */}
      {renderFiveKeyRow([
        k('math', 2, 6, 'dark', 'test', 'A'),
        k('apps', 3, 6, 'dark', 'angle', 'B'),
        k('prgm', 4, 6, 'dark', 'draw', 'C'),
        k('vars', 5, 6, 'dark', 'distr'),
        k('clear', 6, 6),
      ], CONTROL_ROW_HEIGHT)}

      {/* Row 5: x⁻¹, sin, cos, tan, ^ */}
      {renderFiveKeyRow([
        k('x⁻¹', 2, 5, 'dark', 'matrix'),
        k('sin', 3, 5, 'dark', 'sin⁻¹', 'E'),
        k('cos', 4, 5, 'dark', 'cos⁻¹', 'F'),
        k('tan', 5, 5, 'dark', 'tan⁻¹', 'G'),
        k('^', 6, 5, 'dark', 'π', 'H'),
      ], CONTROL_ROW_HEIGHT)}

      {/* Row 6: x², ,, (, ), ÷ */}
      {renderFiveKeyRow([
        k('x²', 2, 4, 'dark', '√'),
        k(',', 3, 4, 'dark', 'EE', 'J'),
        k('(', 4, 4, 'dark', '{', 'K'),
        k(')', 5, 4, 'dark', '}', 'L'),
        k('÷', 6, 4, 'white', 'e', 'M'),
      ], CONTROL_ROW_HEIGHT)}

      {/* Number block: 5 equal columns matching rows above */}
      <div style={{ display: 'flex', gap: COLUMN_SPACING }}>
        {/* Column 1: log, ln, sto→, on */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: ROW_SPACING }}>
          {renderKey(k('log', 2, 3, 'dark', '10ˣ', 'N'), '100%', SIDE_BUTTON_HEIGHT)}
          {renderKey(k('ln', 2, 2, 'dark', 'eˣ', 'S'), '100%', SIDE_BUTTON_HEIGHT)}
          {renderKey(k('sto→', 2, 1, 'dark', 'rcl', 'X'), '100%', SIDE_BUTTON_HEIGHT)}
          {renderKey(k('on', 2, 0, 'dark', 'off'), '100%', SIDE_BUTTON_HEIGHT)}
        </div>

        {/* Column 2: 7, 4, 1, 0 */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: ROW_SPACING }}>
          {renderKey(k('7', 3, 3, 'white', 'u', 'O'), '100%', NUMBER_BUTTON_HEIGHT)}
          {renderKey(k('4', 3, 2, 'white', 'L4', 'T'), '100%', NUMBER_BUTTON_HEIGHT)}
          {renderKey(k('1', 3, 1, 'white', 'L1', 'Y'), '100%', NUMBER_BUTTON_HEIGHT)}
          {renderKey(k('0', 3, 0, 'white', 'catalog'), '100%', NUMBER_BUTTON_HEIGHT)}
        </div>

        {/* Column 3: 8, 5, 2, . */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: ROW_SPACING }}>
          {renderKey(k('8', 4, 3, 'white', 'v', 'P'), '100%', NUMBER_BUTTON_HEIGHT)}
          {renderKey(k('5', 4, 2, 'white', 'L5', 'U'), '100%', NUMBER_BUTTON_HEIGHT)}
          {renderKey(k('2', 4, 1, 'white', 'L2', 'Z'), '100%', NUMBER_BUTTON_HEIGHT)}
          {renderKey(k('.', 4, 0, 'white', 'i', ':'), '100%', NUMBER_BUTTON_HEIGHT)}
        </div>

        {/* Column 4: 9, 6, 3, (-) */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: ROW_SPACING }}>
          {renderKey(k('9', 5, 3, 'white', 'w', 'Q'), '100%', NUMBER_BUTTON_HEIGHT)}
          {renderKey(k('6', 5, 2, 'white', 'L6', 'V'), '100%', NUMBER_BUTTON_HEIGHT)}
          {renderKey(k('3', 5, 1, 'white', 'L3', 'θ'), '100%', NUMBER_BUTTON_HEIGHT)}
          {renderKey(k('(−)', 5, 0, 'white', 'ans', '?'), '100%', NUMBER_BUTTON_HEIGHT)}
        </div>

        {/* Column 5: ×, −, +, enter */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: ROW_SPACING }}>
          {renderKey(k('×', 6, 3, 'white', '[', 'R'), '100%', SIDE_BUTTON_HEIGHT)}
          {renderKey(k('−', 6, 2, 'white', ']', 'W'), '100%', SIDE_BUTTON_HEIGHT)}
          {renderKey(k('+', 6, 1, 'white', 'mem', '"'), '100%', SIDE_BUTTON_HEIGHT)}
          {renderKey(k('enter', 6, 0, 'blue', 'entry', 'solve'), '100%', SIDE_BUTTON_HEIGHT)}
        </div>
      </div>
    </div>
  );
}
