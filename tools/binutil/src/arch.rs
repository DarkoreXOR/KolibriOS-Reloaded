//! x86 bitness / address-size helpers shared by asm and disasm.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bits {
    B16,
    B32,
    B64,
}

impl Bits {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "16" => Ok(Self::B16),
            "32" => Ok(Self::B32),
            "64" => Ok(Self::B64),
            other => Err(format!("invalid --bits '{other}' (want 16|32|64)")),
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            Self::B16 => 16,
            Self::B32 => 32,
            Self::B64 => 64,
        }
    }
}

impl Default for Bits {
    fn default() -> Self {
        Self::B32
    }
}
