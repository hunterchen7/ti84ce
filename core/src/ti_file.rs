//! TI-84 Plus CE file format parser (.8xp, .8xv, .8xg, etc.)
//!
//! Parses the **TI83F* binary format used for calculator variable files.
//! These files contain programs (.8xp), appvars (.8xv), and other variable types.
//!
//! File format:
//!   [55-byte header] [variable entries...] [2-byte checksum]
//!
//! Reference: CEmu core/usb/dusb.c, WikiTI documentation

/// Magic signature at the start of every TI 8x file
const MAGIC: &[u8; 8] = b"**TI83F*";

/// Minimum file size: 55 header + 17 min entry + 2 checksum
const MIN_FILE_SIZE: usize = 74;

/// Variable type codes (from CEmu core/vat.h)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VarType {
    RealNumber = 0x00,
    RealList = 0x01,
    Matrix = 0x02,
    Equation = 0x03,
    String = 0x04,
    Program = 0x05,
    ProtectedProgram = 0x06,
    Picture = 0x07,
    Gdb = 0x08,
    Complex = 0x0C,
    ComplexList = 0x0D,
    AppVar = 0x15,
    Group = 0x17,
    Os = 0x23,
    FlashApp = 0x24,
    Unknown(u8),
}

impl From<u8> for VarType {
    fn from(val: u8) -> Self {
        match val {
            0x00 => VarType::RealNumber,
            0x01 => VarType::RealList,
            0x02 => VarType::Matrix,
            0x03 => VarType::Equation,
            0x04 => VarType::String,
            0x05 => VarType::Program,
            0x06 => VarType::ProtectedProgram,
            0x07 => VarType::Picture,
            0x08 => VarType::Gdb,
            0x0C => VarType::Complex,
            0x0D => VarType::ComplexList,
            0x15 => VarType::AppVar,
            0x17 => VarType::Group,
            0x23 => VarType::Os,
            0x24 => VarType::FlashApp,
            other => VarType::Unknown(other),
        }
    }
}

impl VarType {
    pub fn as_u8(&self) -> u8 {
        match self {
            VarType::RealNumber => 0x00,
            VarType::RealList => 0x01,
            VarType::Matrix => 0x02,
            VarType::Equation => 0x03,
            VarType::String => 0x04,
            VarType::Program => 0x05,
            VarType::ProtectedProgram => 0x06,
            VarType::Picture => 0x07,
            VarType::Gdb => 0x08,
            VarType::Complex => 0x0C,
            VarType::ComplexList => 0x0D,
            VarType::AppVar => 0x15,
            VarType::Group => 0x17,
            VarType::Os => 0x23,
            VarType::FlashApp => 0x24,
            VarType::Unknown(v) => *v,
        }
    }

    /// Whether this type is a program (regular or protected)
    pub fn is_program(&self) -> bool {
        matches!(self, VarType::Program | VarType::ProtectedProgram)
    }

    /// Whether this type is an AppVar
    pub fn is_appvar(&self) -> bool {
        matches!(self, VarType::AppVar)
    }
}

/// A parsed variable entry from a TI file
#[derive(Debug, Clone)]
pub struct TiVarEntry {
    /// Variable type
    pub var_type: VarType,
    /// Variable name (up to 8 bytes, padded with 0x00)
    pub name: [u8; 8],
    /// Version byte
    pub version: u8,
    /// Whether the variable should be archived (flag bit 7)
    pub archived: bool,
    /// Raw variable data (includes the 2-byte size prefix for programs/appvars)
    pub data: Vec<u8>,
}

impl TiVarEntry {
    /// Get the variable name as a string (trimmed of null padding)
    pub fn name_str(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(8);
        std::str::from_utf8(&self.name[..len]).unwrap_or("???")
    }

    /// Get the length of the variable name (excluding null padding)
    pub fn name_len(&self) -> usize {
        self.name.iter().position(|&b| b == 0).unwrap_or(8)
    }

    /// Check if this is an assembly program (starts with 0xEF 0x7B after the 2-byte size)
    pub fn is_asm_program(&self) -> bool {
        self.var_type.is_program() && self.data.len() >= 4
            && self.data[2] == 0xEF && self.data[3] == 0x7B
    }
}

/// A parsed TI file containing one or more variable entries
#[derive(Debug, Clone)]
pub struct TiFile {
    pub entries: Vec<TiVarEntry>,
}

/// Errors that can occur during parsing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TiFileError {
    TooShort,
    BadMagic,
    TruncatedEntry,
    BadChecksum { expected: u16, actual: u16 },
}

impl std::fmt::Display for TiFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TiFileError::TooShort => write!(f, "file too short"),
            TiFileError::BadMagic => write!(f, "bad magic (expected **TI83F*)"),
            TiFileError::TruncatedEntry => write!(f, "truncated variable entry"),
            TiFileError::BadChecksum { expected, actual } => {
                write!(f, "bad checksum: expected 0x{:04X}, got 0x{:04X}", expected, actual)
            }
        }
    }
}

impl TiFile {
    /// Parse a TI 8x file from raw bytes.
    /// Supports .8xp (programs), .8xv (appvars), and other TI83F format files.
    pub fn parse(data: &[u8]) -> Result<Self, TiFileError> {
        if data.len() < MIN_FILE_SIZE {
            return Err(TiFileError::TooShort);
        }

        // Validate magic signature
        if &data[0..8] != MAGIC {
            return Err(TiFileError::BadMagic);
        }

        // Read data section length at offset 53 (2 bytes LE)
        let data_len = u16::from_le_bytes([data[53], data[54]]) as usize;

        // Validate file is long enough: 55 header + data_len + 2 checksum
        if data.len() < 55 + data_len + 2 {
            return Err(TiFileError::TooShort);
        }

        // Verify checksum (lower 16 bits of sum of all bytes in the data section)
        let data_section = &data[55..55 + data_len];
        let computed_sum: u16 = data_section.iter().fold(0u16, |acc, &b| acc.wrapping_add(b as u16));
        let stored_sum = u16::from_le_bytes([data[55 + data_len], data[55 + data_len + 1]]);
        if computed_sum != stored_sum {
            return Err(TiFileError::BadChecksum {
                expected: stored_sum,
                actual: computed_sum,
            });
        }

        // Parse variable entries starting at offset 55
        let mut offset = 55usize;
        let data_end = 55 + data_len;
        let mut entries = Vec::new();

        while offset + 17 <= data_end {
            // Header size (should be 0x0D = 13 for standard entries)
            let _header_size = u16::from_le_bytes([data[offset], data[offset + 1]]);
            // Data size
            let var_data_len = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;
            // Type byte
            let var_type = VarType::from(data[offset + 4]);
            // Name (8 bytes)
            let mut name = [0u8; 8];
            name.copy_from_slice(&data[offset + 5..offset + 13]);
            // Version
            let version = data[offset + 13];
            // Flag (bit 7 = archived)
            let flag = data[offset + 14];
            let archived = (flag & 0x80) != 0;
            // Skip duplicate data size at offset+15..+17

            // Extract variable data
            let var_data_start = offset + 17;
            let var_data_end = var_data_start + var_data_len;
            if var_data_end > data_end {
                return Err(TiFileError::TruncatedEntry);
            }

            entries.push(TiVarEntry {
                var_type,
                name,
                version,
                archived,
                data: data[var_data_start..var_data_end].to_vec(),
            });

            offset = var_data_end;
        }

        Ok(TiFile { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid .8xp file from components
    fn make_8xp(var_type: u8, name: &[u8; 8], version: u8, flag: u8, var_data: &[u8]) -> Vec<u8> {
        let mut file = Vec::new();
        // Header (55 bytes)
        file.extend_from_slice(b"**TI83F*"); // magic
        file.extend_from_slice(&[0x1A, 0x0A, 0x00]); // signature2 + product ID
        file.extend_from_slice(&[0u8; 42]); // comment
        // Data section: entry header (17 bytes) + var_data
        let entry_len = 17 + var_data.len();
        file.extend_from_slice(&(entry_len as u16).to_le_bytes()); // data length
        // Variable entry
        file.extend_from_slice(&13u16.to_le_bytes()); // header size = 0x0D
        file.extend_from_slice(&(var_data.len() as u16).to_le_bytes()); // data size
        file.push(var_type);
        file.extend_from_slice(name);
        file.push(version);
        file.push(flag);
        file.extend_from_slice(&(var_data.len() as u16).to_le_bytes()); // data size (dup)
        file.extend_from_slice(var_data);
        // Checksum (lower 16 bits of sum of bytes from offset 55 to end of data section)
        let checksum: u16 = file[55..].iter().fold(0u16, |acc, &b| acc.wrapping_add(b as u16));
        file.extend_from_slice(&checksum.to_le_bytes());
        file
    }

    #[test]
    fn test_parse_minimal_program() {
        let name = *b"TEST\0\0\0\0";
        // Program data: 2-byte size + ASM marker + NOP
        let var_data = [0x03, 0x00, 0xEF, 0x7B, 0x00];
        let file = make_8xp(0x05, &name, 0, 0, &var_data);

        let parsed = TiFile::parse(&file).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        let entry = &parsed.entries[0];
        assert_eq!(entry.var_type, VarType::Program);
        assert_eq!(entry.name_str(), "TEST");
        assert_eq!(entry.name_len(), 4);
        assert!(!entry.archived);
        assert_eq!(entry.version, 0);
        assert!(entry.is_asm_program());
        assert_eq!(entry.data, var_data);
    }

    #[test]
    fn test_parse_archived_appvar() {
        let name = *b"GRAPHX\0\0";
        let var_data = [0x10, 0x00, 0x01, 0x02, 0x03, 0x04]; // some data
        let file = make_8xp(0x15, &name, 0, 0x80, &var_data);

        let parsed = TiFile::parse(&file).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        let entry = &parsed.entries[0];
        assert_eq!(entry.var_type, VarType::AppVar);
        assert_eq!(entry.name_str(), "GRAPHX");
        assert!(entry.archived);
        assert!(entry.var_type.is_appvar());
        assert!(!entry.is_asm_program());
    }

    #[test]
    fn test_parse_protected_program() {
        let name = *b"DOOM\0\0\0\0";
        let var_data = [0x02, 0x00, 0xEF, 0x7B];
        let file = make_8xp(0x06, &name, 0, 0, &var_data);

        let parsed = TiFile::parse(&file).unwrap();
        let entry = &parsed.entries[0];
        assert_eq!(entry.var_type, VarType::ProtectedProgram);
        assert!(entry.var_type.is_program());
        assert!(entry.is_asm_program());
    }

    #[test]
    fn test_reject_bad_magic() {
        let mut file = make_8xp(0x05, b"TEST\0\0\0\0", 0, 0, &[0, 0]);
        file[0] = b'X';
        assert!(matches!(TiFile::parse(&file), Err(TiFileError::BadMagic)));
    }

    #[test]
    fn test_reject_too_short() {
        assert!(matches!(TiFile::parse(&[0; 10]), Err(TiFileError::TooShort)));
    }

    #[test]
    fn test_reject_bad_checksum() {
        let mut file = make_8xp(0x05, b"TEST\0\0\0\0", 0, 0, &[0, 0]);
        // Corrupt the last byte (checksum)
        let len = file.len();
        file[len - 1] ^= 0xFF;
        match TiFile::parse(&file) {
            Err(TiFileError::BadChecksum { .. }) => {}
            other => panic!("expected BadChecksum, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_real_doom_8xp() {
        // Test with actual DOOM.8xp bytes if available at /tmp/DOOM.8xp
        let path = "/tmp/DOOM.8xp";
        if let Ok(data) = std::fs::read(path) {
            let parsed = TiFile::parse(&data).expect("DOOM.8xp should parse successfully");
            assert_eq!(parsed.entries.len(), 1, "DOOM.8xp should have 1 entry");
            let entry = &parsed.entries[0];
            assert_eq!(entry.name_str(), "DOOM");
            assert!(entry.var_type.is_program(), "DOOM should be a program");
            assert!(entry.is_asm_program(), "DOOM should be an ASM program");
            assert!(!entry.archived, "DOOM.8xp should not be archived");
            // Data should be ~13KB
            assert!(entry.data.len() > 10000, "DOOM data should be >10KB");
            assert!(entry.data.len() < 15000, "DOOM data should be <15KB");
        }
    }

    #[test]
    fn test_parse_real_graphx_8xv() {
        // Test with actual graphx.8xv if available
        let path = "/tmp/clibs_extracted/clibs/graphx.8xv";
        if let Ok(data) = std::fs::read(path) {
            let parsed = TiFile::parse(&data).expect("graphx.8xv should parse successfully");
            assert_eq!(parsed.entries.len(), 1, "graphx.8xv should have 1 entry");
            let entry = &parsed.entries[0];
            assert_eq!(entry.name_str(), "GRAPHX");
            assert_eq!(entry.var_type, VarType::AppVar);
            assert!(entry.archived, "graphx should be archived by default");
        }
    }

    #[test]
    fn test_parse_real_keypadc_8xv() {
        let path = "/tmp/clibs_extracted/clibs/keypadc.8xv";
        if let Ok(data) = std::fs::read(path) {
            let parsed = TiFile::parse(&data).expect("keypadc.8xv should parse successfully");
            assert_eq!(parsed.entries.len(), 1);
            let entry = &parsed.entries[0];
            assert_eq!(entry.name_str(), "KEYPADC");
            assert_eq!(entry.var_type, VarType::AppVar);
            assert!(entry.archived);
        }
    }
}
