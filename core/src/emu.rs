//! Emulator orchestrator
//!
//! For Milestone 1, this is a dummy implementation that draws an animated
//! gradient and responds to key presses visually.

/// TI-84 Plus CE screen dimensions
pub const SCREEN_WIDTH: usize = 320;
pub const SCREEN_HEIGHT: usize = 240;

/// Keypad dimensions (8x8 matrix)
const KEY_ROWS: usize = 8;
const KEY_COLS: usize = 8;

/// Main emulator state
pub struct Emu {
    /// Framebuffer in ARGB8888 format
    framebuffer: Vec<u32>,

    /// Total cycles executed (used for animation)
    tick_counter: u64,

    /// Keypad state matrix (true = pressed)
    key_state: [[bool; KEY_COLS]; KEY_ROWS],

    /// Count of currently pressed keys (for visual effect)
    keys_pressed: u32,

    /// ROM loaded flag
    rom_loaded: bool,

    /// ROM data (stored but not used in Milestone 1)
    rom_data: Vec<u8>,
}

impl Emu {
    /// Create a new emulator instance
    pub fn new() -> Self {
        let mut emu = Self {
            framebuffer: vec![0xFF000000; SCREEN_WIDTH * SCREEN_HEIGHT],
            tick_counter: 0,
            key_state: [[false; KEY_COLS]; KEY_ROWS],
            keys_pressed: 0,
            rom_loaded: false,
            rom_data: Vec::new(),
        };
        emu.render_frame();
        emu
    }

    /// Load ROM data
    pub fn load_rom(&mut self, data: &[u8]) -> Result<(), i32> {
        if data.is_empty() {
            return Err(-2); // Empty ROM
        }

        self.rom_data = data.to_vec();
        self.rom_loaded = true;
        self.reset();
        Ok(())
    }

    /// Reset emulator to initial state
    pub fn reset(&mut self) {
        self.tick_counter = 0;
        self.key_state = [[false; KEY_COLS]; KEY_ROWS];
        self.keys_pressed = 0;
        self.render_frame();
    }

    /// Run for specified cycles, returns cycles executed
    pub fn run_cycles(&mut self, cycles: u32) -> u32 {
        self.tick_counter = self.tick_counter.wrapping_add(cycles as u64);
        self.render_frame();
        cycles
    }

    /// Get framebuffer dimensions
    pub fn framebuffer_size(&self) -> (usize, usize) {
        (SCREEN_WIDTH, SCREEN_HEIGHT)
    }

    /// Get raw pointer to framebuffer
    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }

    /// Set key state
    pub fn set_key(&mut self, row: usize, col: usize, down: bool) {
        if row < KEY_ROWS && col < KEY_COLS {
            let was_pressed = self.key_state[row][col];
            self.key_state[row][col] = down;

            // Update pressed count
            if down && !was_pressed {
                self.keys_pressed += 1;
            } else if !down && was_pressed {
                self.keys_pressed = self.keys_pressed.saturating_sub(1);
            }
        }
    }

    /// Get save state size (stub for Milestone 1)
    pub fn save_state_size(&self) -> usize {
        // Placeholder size
        1024
    }

    /// Save state to buffer (stub for Milestone 1)
    pub fn save_state(&self, _buffer: &mut [u8]) -> Result<usize, i32> {
        // Not implemented in Milestone 1
        Err(-100)
    }

    /// Load state from buffer (stub for Milestone 1)
    pub fn load_state(&mut self, _buffer: &[u8]) -> Result<(), i32> {
        // Not implemented in Milestone 1
        Err(-100)
    }

    /// Render the current frame to the framebuffer
    fn render_frame(&mut self) {
        let time = (self.tick_counter / 1000) as u32;

        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let pixel = self.compute_pixel(x, y, time);
                self.framebuffer[y * SCREEN_WIDTH + x] = pixel;
            }
        }

        // Draw key indicators at bottom
        self.draw_key_indicators();

        // Draw status bar at top
        self.draw_status_bar();
    }

    /// Compute pixel color for animated gradient
    fn compute_pixel(&self, x: usize, y: usize, time: u32) -> u32 {
        // Base gradient colors shift with time
        let time_offset = time % 512;

        // Create animated gradient
        let r = ((x + time_offset as usize) % 256) as u8;
        let g = ((y + time_offset as usize / 2) % 256) as u8;
        let b = ((x + y + time_offset as usize) % 256) as u8;

        // Modify colors based on key press count
        let key_effect = (self.keys_pressed * 30).min(150) as u8;
        let r = r.saturating_add(key_effect);
        let g = g.saturating_sub(key_effect / 2);

        // ARGB8888 format
        0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    }

    /// Draw key state indicators at bottom of screen
    fn draw_key_indicators(&mut self) {
        let indicator_size = 8;
        let start_x = 10;
        let start_y = SCREEN_HEIGHT - 20;
        let spacing = 12;

        for row in 0..KEY_ROWS {
            for col in 0..KEY_COLS {
                let x = start_x + col * spacing;
                let y = start_y + (row / 4) * spacing;

                // Only draw first 2 rows of indicators to fit
                if row >= 4 {
                    continue;
                }

                let color = if self.key_state[row][col] {
                    0xFF00FF00 // Green for pressed
                } else {
                    0xFF404040 // Dark gray for released
                };

                // Draw small square
                for dy in 0..indicator_size {
                    for dx in 0..indicator_size {
                        let px = x + dx;
                        let py = y + dy;
                        if px < SCREEN_WIDTH && py < SCREEN_HEIGHT {
                            self.framebuffer[py * SCREEN_WIDTH + px] = color;
                        }
                    }
                }
            }
        }
    }

    /// Draw status bar at top of screen
    fn draw_status_bar(&mut self) {
        let bar_height = 20;

        // Draw dark background for status bar
        for y in 0..bar_height {
            for x in 0..SCREEN_WIDTH {
                self.framebuffer[y * SCREEN_WIDTH + x] = 0xFF202020;
            }
        }

        // Draw ROM status indicator
        let rom_color = if self.rom_loaded {
            0xFF00FF00 // Green
        } else {
            0xFFFF0000 // Red
        };

        // ROM indicator box
        for y in 4..16 {
            for x in 4..16 {
                self.framebuffer[y * SCREEN_WIDTH + x] = rom_color;
            }
        }

        // Draw tick counter visualization (simple bar)
        let bar_width = ((self.tick_counter / 10000) % (SCREEN_WIDTH as u64 - 40)) as usize;
        for y in 6..14 {
            for x in 24..(24 + bar_width) {
                self.framebuffer[y * SCREEN_WIDTH + x] = 0xFF00AAFF;
            }
        }

        // Draw keys pressed count as boxes
        let key_count_start = SCREEN_WIDTH - 80;
        for i in 0..self.keys_pressed.min(8) as usize {
            for y in 4..16 {
                for x in (key_count_start + i * 10)..(key_count_start + i * 10 + 8) {
                    if x < SCREEN_WIDTH {
                        self.framebuffer[y * SCREEN_WIDTH + x] = 0xFFFFFF00;
                    }
                }
            }
        }
    }
}

impl Default for Emu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_emu() {
        let emu = Emu::new();
        assert_eq!(emu.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert!(!emu.rom_loaded);
    }

    #[test]
    fn test_load_rom() {
        let mut emu = Emu::new();
        let rom = vec![0u8; 1024];
        assert!(emu.load_rom(&rom).is_ok());
        assert!(emu.rom_loaded);
    }

    #[test]
    fn test_empty_rom_fails() {
        let mut emu = Emu::new();
        let rom: Vec<u8> = vec![];
        assert!(emu.load_rom(&rom).is_err());
    }

    #[test]
    fn test_key_state() {
        let mut emu = Emu::new();

        emu.set_key(0, 0, true);
        assert!(emu.key_state[0][0]);
        assert_eq!(emu.keys_pressed, 1);

        emu.set_key(0, 0, false);
        assert!(!emu.key_state[0][0]);
        assert_eq!(emu.keys_pressed, 0);
    }

    #[test]
    fn test_run_cycles() {
        let mut emu = Emu::new();
        let executed = emu.run_cycles(1000);
        assert_eq!(executed, 1000);
        assert_eq!(emu.tick_counter, 1000);
    }

    #[test]
    fn test_reset() {
        let mut emu = Emu::new();
        emu.run_cycles(5000);
        emu.set_key(1, 1, true);
        emu.reset();

        assert_eq!(emu.tick_counter, 0);
        assert!(!emu.key_state[1][1]);
        assert_eq!(emu.keys_pressed, 0);
    }
}
