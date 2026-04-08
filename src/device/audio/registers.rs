/// Symbolic register names for the audio MMIO block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioRegister {
    // Voice 0
    V0FreqLo = 0x00,
    V0FreqHi = 0x01,
    V0PwLo = 0x02,
    V0PwHi = 0x03,
    V0Control = 0x04,
    V0Ad = 0x05,
    V0Sr = 0x06,

    // Voice 1
    V1FreqLo = 0x07,
    V1FreqHi = 0x08,
    V1PwLo = 0x09,
    V1PwHi = 0x0A,
    V1Control = 0x0B,
    V1Ad = 0x0C,
    V1Sr = 0x0D,

    // Voice 2
    V2FreqLo = 0x0E,
    V2FreqHi = 0x0F,
    V2PwLo = 0x10,
    V2PwHi = 0x11,
    V2Control = 0x12,
    V2Ad = 0x13,
    V2Sr = 0x14,

    // Filter and volume
    FilterCutoffLo = 0x15,
    FilterCutoffHi = 0x16,
    FilterResMode = 0x17,
    FilterModeVolume = 0x18,

    // Readback (read-only)
    Osc3Read = 0x1B,
    Env3Read = 0x1C,
}

impl AudioRegister {
    /// Decodes a raw register offset into a symbolic register name.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(AudioRegister::V0FreqLo),
            0x01 => Some(AudioRegister::V0FreqHi),
            0x02 => Some(AudioRegister::V0PwLo),
            0x03 => Some(AudioRegister::V0PwHi),
            0x04 => Some(AudioRegister::V0Control),
            0x05 => Some(AudioRegister::V0Ad),
            0x06 => Some(AudioRegister::V0Sr),
            0x07 => Some(AudioRegister::V1FreqLo),
            0x08 => Some(AudioRegister::V1FreqHi),
            0x09 => Some(AudioRegister::V1PwLo),
            0x0A => Some(AudioRegister::V1PwHi),
            0x0B => Some(AudioRegister::V1Control),
            0x0C => Some(AudioRegister::V1Ad),
            0x0D => Some(AudioRegister::V1Sr),
            0x0E => Some(AudioRegister::V2FreqLo),
            0x0F => Some(AudioRegister::V2FreqHi),
            0x10 => Some(AudioRegister::V2PwLo),
            0x11 => Some(AudioRegister::V2PwHi),
            0x12 => Some(AudioRegister::V2Control),
            0x13 => Some(AudioRegister::V2Ad),
            0x14 => Some(AudioRegister::V2Sr),
            0x15 => Some(AudioRegister::FilterCutoffLo),
            0x16 => Some(AudioRegister::FilterCutoffHi),
            0x17 => Some(AudioRegister::FilterResMode),
            0x18 => Some(AudioRegister::FilterModeVolume),
            0x1B => Some(AudioRegister::Osc3Read),
            0x1C => Some(AudioRegister::Env3Read),
            _ => None,
        }
    }

    /// Returns the raw register offset.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}
