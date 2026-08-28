// src/apu/mod.rs

//! APU module with a very small, fast-to-implement square channel for audio
//! generation and register mirroring. This is intentionally minimal: it
//! supports register reads/writes for FF10-FF3F and generates a single
//! square-channel-derived audio stream when enabled.

const BASE_ADDR: u16 = 0xFF10;

pub struct Apu {
    /// MMIO registers for the APU region (FF10..FF3F) - 0x30 bytes
    regs: [u8; 0x30],
    /// Linear PCM sample buffer (mono) in -1.0..1.0
    pub sample_buffer: Vec<f32>,
    tick_counter: u64,

    // Simple channel 1 state (square 1) ------------------------------
    phase: f64,
    sample_accumulator: f64,
}

impl Apu {
    pub fn new() -> Self {
        Apu {
            regs: [0u8; 0x30],
            sample_buffer: Vec::new(),
            tick_counter: 0,
            phase: 0.0,
            sample_accumulator: 0.0,
        }
    }

    /// Advance the APU by `cycles` CPU cycles and produce audio samples
    /// at ~44100 Hz into sample_buffer. This implementation only uses
    /// channel 1 (square) registers (NR10..NR14) to produce a simple
    /// audible output when enabled.
    pub fn tick(&mut self, cycles: u32) {
        // Clock advancement
        self.tick_counter = self.tick_counter.wrapping_add(cycles as u64);

        // Convert cycles to seconds using Game Boy clock (4_194_304 Hz)
        let seconds = (cycles as f64) / 4_194_304.0;
        if seconds <= 0.0 { return; }

        // Sample rate and number of samples to produce
        let sample_rate = 44100.0_f64;
        let samples_f = seconds * sample_rate;
        self.sample_accumulator += samples_f;
        let to_emit = self.sample_accumulator.floor() as usize;
        self.sample_accumulator -= to_emit as f64;

        if to_emit == 0 { return; }

        // Read channel 1 registers (NR10..NR14)
        let nr10 = self.regs[(0x00) as usize]; // sweep (unused)
        let nr11 = self.regs[(0x01) as usize]; // duty/length
        let nr12 = self.regs[(0x02) as usize]; // envelope
        let nr13 = self.regs[(0x03) as usize]; // freq L
        let nr14 = self.regs[(0x04) as usize]; // freq H / control

        // Determine if channel 1 is enabled by checking bit 7 of NR14's "trigger"/enable
        let ch1_enabled = (nr14 & 0x80) != 0;

        // Compute duty: bits 6-7 of NR11 are duty (0..3) -> fraction of high time
        let duty = match (nr11 >> 6) & 0x03 {
            0 => 0.125, // 12.5%
            1 => 0.25,  // 25%
            2 => 0.5,   // 50%
            3 => 0.75,  // 75%
            _ => 0.5,
        };

        // Volume from NR12 upper 4 bits (0..15)
        let volume = ((nr12 >> 4) & 0x0F) as f32 / 15.0f32;

        // Frequency calculation: 131072 / (2048 - x)
        let freq_raw = ((nr14 as u16 & 0x07) << 8) | (nr13 as u16);
        let freq = if freq_raw >= 2048 { 0.0 } else { 131072.0 / (2048u16.saturating_sub(freq_raw) as f64) };

        // Generate to_emit samples
        let mut phase = self.phase;
        for _ in 0..to_emit {
            let sample = if ch1_enabled && freq > 0.0 {
                // advance phase by cycles-per-sample fraction: phase increments by freq/sample_rate
                // We'll use phase in [0,1) representing waveform period
                phase = (phase + (freq / sample_rate)) % 1.0;
                if phase < duty { (volume * 2.0 - 1.0) as f32 } else { (-(volume * 2.0 - 1.0)) as f32 }
            } else {
                0.0_f32
            };
            self.sample_buffer.push(sample);
        }
        self.phase = phase;
    }

    /// Read an APU MMIO register (FF10-FF3F).
    pub fn read_reg(&self, addr: u16) -> u8 {
        let idx = addr.wrapping_sub(BASE_ADDR) as usize;
        if idx < self.regs.len() {
            self.regs[idx]
        } else {
            0
        }
    }

    /// Write to an APU MMIO register (FF10-FF3F).
    pub fn write_reg(&mut self, addr: u16, val: u8) {
        let idx = addr.wrapping_sub(BASE_ADDR) as usize;
        if idx >= self.regs.len() { return; }
        // Mirror the written value into the registers array. Some registers
        // (like NR14's trigger) might cause actions; support a minimal
        // trigger behaviour by keeping the value and letting tick() read it.
        self.regs[idx] = val;
    }

    /// Drain the sample buffer and return its contents.
    pub fn drain_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.sample_buffer)
    }
}
