//! SHA256 Accelerator
//!
//! Memory-mapped at port 0x2xxx (I/O port address space)
//!
//! Register layout (from CEmu sha256.c):
//! - 0x00: Control register (write triggers operations)
//! - 0x0C: state[7] - lowest hash word for quick read
//! - 0x10-0x4F: block[0-15] - 64 bytes of input data (16 x 32-bit words)
//! - 0x60-0x7F: state[0-7] - 32 bytes of hash output (8 x 32-bit words)

/// SHA-256 round constants
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA256 accelerator controller
#[derive(Debug, Clone)]
pub struct Sha256Controller {
    /// Input block (64 bytes / 16 words)
    block: [u32; 16],
    /// Hash state (32 bytes / 8 words)
    state: [u32; 8],
    /// Last accessed index (for protected port behavior)
    last: u16,
}

impl Sha256Controller {
    /// Initial SHA256 state (standard IV)
    const INITIAL_STATE: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    /// Create a new SHA256 controller
    /// CEmu sha256_reset() memsets to 0 (not IV)
    pub fn new() -> Self {
        Self {
            block: [0; 16],
            state: [0; 8],
            last: 0,
        }
    }

    /// Reset the controller
    /// CEmu sha256_reset() memsets entire struct to 0
    pub fn reset(&mut self) {
        self.block = [0; 16];
        self.state = [0; 8];
        self.last = 0;
    }

    /// Process one 64-byte block through SHA-256 compression
    /// Matches CEmu's process_block() in sha256.c
    fn process_block(&mut self) {
        let mut w = [0u32; 64];

        // Copy block into first 16 words
        w[..16].copy_from_slice(&self.block);

        // Extend to 64 words
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        // 64 rounds of compression
        for i in 0..64 {
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    /// Read a byte from the SHA256 registers
    /// addr is offset within 0x2xxx range (0x00-0xFF typically)
    pub fn read(&self, addr: u32) -> u8 {
        let index = (addr >> 2) as usize;
        let bit_offset = ((addr & 3) * 8) as u32;

        if index == 0x0C >> 2 {
            // Quick access to state[7]
            ((self.state[7] >> bit_offset) & 0xFF) as u8
        } else if index >= 0x10 >> 2 && index < 0x50 >> 2 {
            // Block data (0x10-0x4F)
            let block_idx = index - (0x10 >> 2);
            if block_idx < 16 {
                ((self.block[block_idx] >> bit_offset) & 0xFF) as u8
            } else {
                0
            }
        } else if index >= 0x60 >> 2 && index < 0x80 >> 2 {
            // State data (0x60-0x7F)
            let state_idx = index - (0x60 >> 2);
            if state_idx < 8 {
                ((self.state[state_idx] >> bit_offset) & 0xFF) as u8
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Write a byte to the SHA256 registers
    /// addr is offset within 0x2xxx range
    pub fn write(&mut self, addr: u32, value: u8) {
        let index = (addr >> 2) as usize;
        let bit_offset = ((addr & 3) * 8) as u32;

        if addr == 0 {
            // Control register at 0x00
            // CEmu uses independent ifs (not else-if chain):
            //   if (byte & 0x10) { clear state }
            //   else {
            //     if ((byte & 0xE) == 0xA) { initialize (first block) }
            //     if ((byte & 0xA) == 0xA) { process_block (subsequent blocks) }
            //   }
            // Note: 0x0A matches both conditions (init + process = first block hash)
            // 0x0E/0x0F matches only process (subsequent block hash)
            if value & 0x10 != 0 {
                // Clear state to zero
                self.state = [0; 8];
            } else {
                if (value & 0x0E) == 0x0A {
                    // Initialize with IV (first block: 0x0A or 0x0B)
                    self.state = Self::INITIAL_STATE;
                }
                if (value & 0x0A) == 0x0A {
                    // Process block (0x0A, 0x0B, 0x0E, 0x0F)
                    self.process_block();
                }
            }
        } else if index >= 0x10 >> 2 && index < 0x50 >> 2 {
            // Block data (0x10-0x4F)
            let block_idx = index - (0x10 >> 2);
            if block_idx < 16 {
                let mask = !(0xFF << bit_offset);
                self.block[block_idx] = (self.block[block_idx] & mask) | ((value as u32) << bit_offset);
            }
        }
        // State registers are read-only
    }
}

impl Default for Sha256Controller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let sha = Sha256Controller::new();
        // CEmu memsets to 0 on reset, not IV
        assert_eq!(sha.state, [0; 8]);
        assert_eq!(sha.block, [0; 16]);
    }

    #[test]
    fn test_reset() {
        let mut sha = Sha256Controller::new();
        sha.block[0] = 0x12345678;
        sha.state[0] = 0xDEADBEEF;
        sha.reset();
        // CEmu memsets to 0
        assert_eq!(sha.state, [0; 8]);
        assert_eq!(sha.block, [0; 16]);
    }

    #[test]
    fn test_read_state() {
        let mut sha = Sha256Controller::new();
        // Initialize to IV first
        sha.write(0x00, 0x0A);
        // state[7] at 0x0C should be 0x5be0cd19 (from IV, after process_block on zero block)
        // Actually after 0x0A: initialize to IV then process_block on zero block
        // Let's just check state[0] at 0x60
        let s0 = sha.state[0];
        assert_eq!(sha.read(0x60), (s0 & 0xFF) as u8);
        assert_eq!(sha.read(0x61), ((s0 >> 8) & 0xFF) as u8);
        assert_eq!(sha.read(0x62), ((s0 >> 16) & 0xFF) as u8);
        assert_eq!(sha.read(0x63), ((s0 >> 24) & 0xFF) as u8);
    }

    #[test]
    fn test_write_block() {
        let mut sha = Sha256Controller::new();
        // Write to block[0] at 0x10
        sha.write(0x10, 0x78);
        sha.write(0x11, 0x56);
        sha.write(0x12, 0x34);
        sha.write(0x13, 0x12);
        assert_eq!(sha.block[0], 0x12345678);
    }

    #[test]
    fn test_control_initialize_and_process() {
        let mut sha = Sha256Controller::new();
        // Write 0x0A: initializes to IV AND processes block (both conditions match)
        sha.write(0x00, 0x0A);
        // State should be IV + compression of zero block
        // This is NOT just the IV
        assert_ne!(sha.state, Sha256Controller::INITIAL_STATE);
        assert_ne!(sha.state, [0; 8]);
    }

    #[test]
    fn test_control_clear() {
        let mut sha = Sha256Controller::new();
        sha.state[0] = 0xDEADBEEF;
        // Write 0x10 to control to clear state
        sha.write(0x00, 0x10);
        assert_eq!(sha.state, [0; 8]);
    }

    #[test]
    fn test_control_process_only() {
        let mut sha = Sha256Controller::new();
        // Initialize state to IV
        sha.state = Sha256Controller::INITIAL_STATE;
        // Write 0x0E: process block only (no init), matches (byte & 0xA) == 0xA
        sha.write(0x00, 0x0E);
        // State should be different from IV (processed zero block)
        assert_ne!(sha.state, Sha256Controller::INITIAL_STATE);
    }

    #[test]
    fn test_nist_single_block() {
        // NIST test vector: SHA-256("abc")
        // Expected: ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad
        let mut sha = Sha256Controller::new();

        // Prepare the message "abc" as a padded SHA-256 block
        // "abc" = 0x61626380 followed by zeros and length 24 bits = 0x18
        sha.block[0] = 0x61626380;
        for i in 1..15 {
            sha.block[i] = 0;
        }
        sha.block[15] = 0x18; // Length in bits = 24

        // Initialize and process (0x0A = first block)
        sha.write(0x00, 0x0A);

        assert_eq!(sha.state[0], 0xba7816bf);
        assert_eq!(sha.state[1], 0x8f01cfea);
        assert_eq!(sha.state[2], 0x414140de);
        assert_eq!(sha.state[3], 0x5dae2223);
        assert_eq!(sha.state[4], 0xb00361a3);
        assert_eq!(sha.state[5], 0x96177a9c);
        assert_eq!(sha.state[6], 0xb410ff61);
        assert_eq!(sha.state[7], 0xf20015ad);
    }

    #[test]
    fn test_quick_access_state7() {
        let mut sha = Sha256Controller::new();
        sha.state[7] = 0xDEADBEEF;
        // Quick access at 0x0C reads state[7]
        assert_eq!(sha.read(0x0C), 0xEF);
        assert_eq!(sha.read(0x0D), 0xBE);
        assert_eq!(sha.read(0x0E), 0xAD);
        assert_eq!(sha.read(0x0F), 0xDE);
    }
}
