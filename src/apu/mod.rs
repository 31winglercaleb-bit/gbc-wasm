// src/apu/mod.rs

//! APU module skeleton
//!
//! This file adds the top-level APU struct and a minimal API used by the
//! emulator. The detailed channel implementations and MMU register mapping
//! will be added in follow-up commits.

pub struct Apu {
    /// Linear PCM sample buffer (mono) in the range -1.0..1.0
    pub sample_buffer: Vec<f32>,
    tick_counter: u64,
}

impl Apu {
    /// Create a new, uninitialized APU.
    pub fn new() -> Self {
        Apu {
            sample_buffer: Vec::new(),
            tick_counter: 0,
        }
    }

    /// Advance the APU by `cycles` CPU cycles. This is a placeholder
    /// implementation — the frame sequencer and channel stepping will be
    /// implemented incrementally in later commits.
    pub fn tick(&mut self, cycles: u32) {
        self.tick_counter = self.tick_counter.wrapping_add(cycles as u64);
        // TODO: step channels, generate samples and push into sample_buffer
    }

    /// Read an APU MMIO register (FF10-FF3F). Placeholder returns 0.
    pub fn read_reg(&self, _addr: u16) -> u8 {
        0
    }

    /// Write to an APU MMIO register (FF10-FF3F). Placeholder no-op.
    pub fn write_reg(&mut self, _addr: u16, _val: u8) {
        // TODO: implement APU register behavior
    }

    /// Drain the sample buffer and return its contents.
    pub fn drain_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.sample_buffer)
    }
}
