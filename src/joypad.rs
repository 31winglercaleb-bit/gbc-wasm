// src/joypad.rs

//! Joypad (input) module skeleton
//!
//! Provides a minimal Joypad struct with read/write helpers for the P1
//! register. The real implementation will track directional and button
//! state and trigger interrupts on changes.

#[derive(Default)]
pub struct Joypad {
    /// Current low-level button state bitmask (bits 0-3: A/B/Select/Start, 4-7: Right/Left/Up/Down)
    pub buttons: u8,
    /// P1 register selection bits (bits 4-5 in the real hardware)
    pub p1_select: u8,
}

impl Joypad {
    pub fn new() -> Self {
        Joypad { buttons: 0xFF, p1_select: 0x00 }
    }

    /// Set or clear a button bit in the low-level `buttons` mask.
    /// `mask` should be one of 0x01..0x80 depending on mapping chosen.
    pub fn set_button(&mut self, mask: u8, pressed: bool) {
        if pressed {
            self.buttons &= !mask;
        } else {
            self.buttons |= mask;
        }
    }

    /// Read the P1 register (0xFF00) with selection bits applied.
    /// This returns the standard Game Boy semantics value (0 = pressed).
    pub fn read_p1(&self) -> u8 {
        // Very small placeholder implementation: return high nibble | low nibble
        // Real behaviour: bits 4-5 select between direction and button groups.
        (self.p1_select & 0x30) | (self.buttons & 0x0F)
    }

    /// Write to P1 selection bits (only bits 4-5 are writable)
    pub fn write_p1(&mut self, val: u8) {
        self.p1_select = val & 0x30;
    }
}
