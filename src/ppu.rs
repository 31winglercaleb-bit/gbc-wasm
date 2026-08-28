// src/ppu.rs
// Lightweight PPU skeleton that produces an RGBA framebuffer. Not cycle-accurate yet.

pub struct Ppu {
    frame_buffer: Vec<u8>,
    cycles: u32,
}

impl Ppu {
    pub fn new() -> Ppu {
        let mut fb = vec![0u8; 160 * 144 * 4];
        // initial pattern
        for y in 0..144 {
            for x in 0..160 {
                let i = (y * 160 + x) * 4;
                fb[i] = (x % 256) as u8;
                fb[i + 1] = (y % 256) as u8;
                fb[i + 2] = ((x + y) % 256) as u8;
                fb[i + 3] = 0xFF;
            }
        }
        Ppu { frame_buffer: fb, cycles: 0 }
    }

    pub fn step(&mut self, cycles: u32, _mmu: &mut crate::mmu::Mmu) {
        self.cycles = self.cycles.wrapping_add(cycles);
        // Roughly every 70224 cycles is a frame; when exceeded, update the pattern slightly.
        if self.cycles >= 70224 {
            self.cycles = self.cycles.wrapping_sub(70224);
            // simple animation: rotate colors
            for i in (0..self.frame_buffer.len()).step_by(4) {
                let r = self.frame_buffer[i];
                let g = self.frame_buffer[i+1];
                let b = self.frame_buffer[i+2];
                self.frame_buffer[i] = g;
                self.frame_buffer[i+1] = b;
                self.frame_buffer[i+2] = r;
            }
        }
    }

    pub fn render_frame(&mut self) -> Vec<u8> {
        self.frame_buffer.clone()
    }
}
