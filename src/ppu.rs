// src/ppu.rs
// PPU implementation: basic background rendering (tile map) with scanline timing.
// Not fully cycle-accurate, but sufficient to render background tiles and produce
// correct scrolling using SCX/SCY and LCDC registers. Palette is taken from BGP (0xFF47).

pub struct Ppu {
    frame_buffer: Vec<u8>,
    cycles: u32,
    ly: u8,
}

impl Ppu {
    pub fn new() -> Ppu {
        let fb = vec![0u8; 160 * 144 * 4];
        Ppu { frame_buffer: fb, cycles: 0, ly: 0 }
    }

    pub fn step(&mut self, cycles: u32, mmu: &mut crate::mmu::Mmu) {
        // Advance internal cycles and handle scanline progression.
        self.cycles = self.cycles.wrapping_add(cycles);
        const CYCLES_PER_SCANLINE: u32 = 456;

        while self.cycles >= CYCLES_PER_SCANLINE {
            self.cycles -= CYCLES_PER_SCANLINE;
            // Increment LY
            self.ly = self.ly.wrapping_add(1);
            // write LY to 0xFF44
            mmu.write_u8(0xFF44, self.ly);

            if self.ly == 144 {
                // Enter VBlank: request VBlank interrupt (bit 0)
                let iflag = mmu.read_u8(0xFF0F);
                mmu.write_u8(0xFF0F, iflag | 0x01);
                // Render the full frame into framebuffer
                self.render_full_frame(mmu);
            }

            if self.ly > 153 {
                // Restart frame
                self.ly = 0;
                mmu.write_u8(0xFF44, self.ly);
            }
        }
    }

    pub fn render_frame(&mut self) -> Vec<u8> {
        self.frame_buffer.clone()
    }

    fn render_full_frame(&mut self, mmu: &mut crate::mmu::Mmu) {
        // Read LCDC and BGP and SCX/SCY
        let lcdc = mmu.read_u8(0xFF40);
        if (lcdc & 0x80) == 0 {
            // LCD disabled: produce blank white screen
            for i in (0..self.frame_buffer.len()).step_by(4) {
                self.frame_buffer[i] = 0xFF;
                self.frame_buffer[i + 1] = 0xFF;
                self.frame_buffer[i + 2] = 0xFF;
                self.frame_buffer[i + 3] = 0xFF;
            }
            return;
        }

        let scy = mmu.read_u8(0xFF42) as usize;
        let scx = mmu.read_u8(0xFF43) as usize;
        let bg_palette = mmu.read_u8(0xFF47);

        // Background tile map select (LCDC bit 3): 0 = 0x9800, 1 = 0x9C00
        let bg_map_base = if (lcdc & 0x08) != 0 { 0x9C00 } else { 0x9800 };
        // Tile data select (LCDC bit 4): 0 = 0x8800(signed), 1 = 0x8000(unsigned)
        let tile_data_select = (lcdc & 0x10) != 0;

        for y in 0..144 {
            let vy = (y + scy) & 0xFF; // vertical position in background (0..255)
            let tile_row = (vy / 8) & 0x1F; // 32 rows
            let line_in_tile = vy % 8;

            for x in 0..160 {
                let vx = (x + scx) & 0xFF; // horizontal position in background
                let tile_col = (vx / 8) & 0x1F; // 32 cols

                let map_addr = bg_map_base + tile_row * 32 + tile_col;
                let tile_num = mmu.read_u8(map_addr);

                // Determine tile data address
                let tile_addr = if tile_data_select {
                    // unsigned: 0x8000 + tile_num * 16
                    0x8000usize + (tile_num as usize) * 16usize
                } else {
                    // signed: 0x9000 + (i8(tile_num)) * 16
                    let tn = tile_num as i8 as i16;
                    let base = 0x9000i32 as i32; // use i32 for safety
                    (base + (tn as i32) * 16) as usize
                };

                // Each tile line: two bytes (low, high)
                let byte1 = mmu.read_u8(tile_addr + (line_in_tile * 2) as usize);
                let byte2 = mmu.read_u8(tile_addr + (line_in_tile * 2) as usize + 1);

                let bit = 7 - (vx % 8);
                let low = ((byte1 >> bit) & 0x01) as u8;
                let high = ((byte2 >> bit) & 0x01) as u8;
                let color_id = (high << 1) | low; // 0..3

                let shade = match ((bg_palette >> (color_id * 2)) & 0x03) {
                    0 => 0xFFu8, // white
                    1 => 0xC0u8, // light gray
                    2 => 0x60u8, // dark gray
                    3 => 0x00u8, // black
                    _ => 0xFFu8,
                };

                let i = (y * 160 + x) * 4;
                self.frame_buffer[i] = shade;
                self.frame_buffer[i + 1] = shade;
                self.frame_buffer[i + 2] = shade;
                self.frame_buffer[i + 3] = 0xFF;
            }
        }
    }
}
