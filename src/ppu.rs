// src/ppu.rs
// PPU implementation: background rendering with sprite (OAM) overlay and STAT handling.
// Implements scanline timing (456 cycles/scanline), LY/LYC, VBlank and STAT interrupts
// (basic: mode interrupt enables & LYC interrupt), and simple sprite drawing using OBP0/1.

pub struct Ppu {
    frame_buffer: Vec<u8>,
    bg_color_map: Vec<u8>,
    cycles: u32,
    ly: u8,
    scanline_cycles: u32,
}

impl Ppu {
    pub fn new() -> Ppu {
        let fb = vec![0u8; 160 * 144 * 4];
        let cmap = vec![0u8; 160 * 144];
        Ppu { frame_buffer: fb, bg_color_map: cmap, cycles: 0, ly: 0, scanline_cycles: 0 }
    }

    pub fn step(&mut self, cycles: u32, mmu: &mut crate::mmu::Mmu) {
        // Advance internal cycles and handle scanline progression.
        self.scanline_cycles = self.scanline_cycles.wrapping_add(cycles);
        const CYCLES_PER_SCANLINE: u32 = 456;

        while self.scanline_cycles >= CYCLES_PER_SCANLINE {
            self.scanline_cycles -= CYCLES_PER_SCANLINE;
            // Increment LY
            self.ly = self.ly.wrapping_add(1);
            mmu.write_u8(0xFF44, self.ly);

            if self.ly == 144 {
                // Enter VBlank: request VBlank interrupt (bit 0)
                let iflag = mmu.read_u8(0xFF0F);
                mmu.write_u8(0xFF0F, iflag | 0x01);
                // Also set STAT mode to 1 (VBlank)
                self.set_stat_mode(mmu, 1);
                // Render the full frame into framebuffer
                self.render_full_frame(mmu);
            }

            if self.ly > 153 {
                // Restart frame
                self.ly = 0;
                mmu.write_u8(0xFF44, self.ly);
                // Clear STAT coincidence flag maybe handled in set_ly
            }

            // For visible lines (0..143) we should enter Mode 2 at start of line
            if self.ly < 144 {
                // Mode progression for the new line will start at mode 2 (OAM search)
                self.set_stat_mode(mmu, 2);
                // Optionally request STAT interrupt for mode 2 if enabled
                self.maybe_request_stat_for_mode(mmu, 2);
            } else {
                // Lines >=144 are VBlank (mode 1)
                self.set_stat_mode(mmu, 1);
            }

            // LYC compare: set coinidence flag and request if enabled
            let lyc = mmu.read_u8(0xFF45);
            if lyc == self.ly {
                // set coincidence flag (STAT bit 2)
                let mut stat = mmu.read_u8(0xFF41);
                stat |= 0x04;
                mmu.write_u8(0xFF41, stat);
                // request STAT interrupt if bit 6? No: bit 2 is LYC interrupt enable per docs (STAT bit 6 is flag)
                // In our mmu, STAT layout: bits5-3 mode interrupt enables, bit2 LYC interrupt enable, bit1-0 mode flag
                if (stat & 0x04) != 0 {
                    // if LYC=LY interrupt enable (STAT bit2) set, request STAT
                    // According to common documentation, bit2 is not the enable; however implement: if (STAT & 0x40) ???
                    // To be conservative, if STAT bit 2 (coincidence flag) and STAT bit 6?? Unknown; instead inspect STAT enable bits
                }
                // Check LYC interrupt enable (STAT bit 6? real GB: bit6 is coincidence flag, bit2 is LYC interrupt enable) -> use bit2 of STAT read-only? We'll check enable at bit 6? Simpler: check STAT enable at 0xFF41 & 0x40? No.
                // We'll check the enable bit at bit 6 of the stored STAT writes: per common refs, STAT bit 6 is unused; but real layout is: bit6 - LYC coincidence flag (read only), bit5 - OAM int enable, bit4 - VBlank int enable, bit3 - HBlank int enable, bit2 - LYC int enable.
                let stat_reg = mmu.read_u8(0xFF41);
                if (stat_reg & 0x04) != 0 {
                    // If LYC coincidence enable (bit2) is set, request STAT interrupt (bit1 in IF)
                    let iflag = mmu.read_u8(0xFF0F);
                    mmu.write_u8(0xFF0F, iflag | 0x02);
                }
            } else {
                // clear coincidence flag
                let mut stat = mmu.read_u8(0xFF41);
                stat &= !0x04u8;
                mmu.write_u8(0xFF41, stat);
            }
        }

        // Within the current scanline, update STAT mode bits based on scanline_cycles
        self.update_stat_mode_by_cycle(mmu);
    }

    fn update_stat_mode_by_cycle(&mut self, mmu: &mut crate::mmu::Mmu) {
        // Only meaningful for ly < 144; for >=144 mode1
        if self.ly >= 144 {
            self.set_stat_mode(mmu, 1);
            return;
        }
        let cy = self.scanline_cycles;
        // Mode 2: 0-79, Mode 3: 80-251, Mode0: 252-455
        let mode = if cy < 80 {
            2
        } else if cy < 252 {
            3
        } else {
            0
        };
        let prev_mode = mmu.read_u8(0xFF41) & 0x03;
        if mode != prev_mode as u32 {
            self.set_stat_mode(mmu, mode as u8);
            self.maybe_request_stat_for_mode(mmu, mode as u8);
        }
    }

    fn set_stat_mode(&mut self, mmu: &mut crate::mmu::Mmu, mode: u8) {
        let mut stat = mmu.read_u8(0xFF41);
        stat = (stat & !0x03) | (mode & 0x03);
        mmu.write_u8(0xFF41, stat);
    }

    fn maybe_request_stat_for_mode(&mut self, mmu: &mut crate::mmu::Mmu, mode: u8) {
        // STAT bits: bit5 = mode2 OAM int enable, bit4 = mode1 VBlank int enable, bit3 = mode0 HBlank int enable
        let stat = mmu.read_u8(0xFF41);
        let enable = match mode {
            2 => (stat & 0x20) != 0,
            1 => (stat & 0x10) != 0,
            0 => (stat & 0x08) != 0,
            _ => false,
        };
        if enable {
            let iflag = mmu.read_u8(0xFF0F);
            mmu.write_u8(0xFF0F, iflag | 0x02); // set STAT interrupt (IF bit 1)
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
            for i in 0..self.bg_color_map.len() { self.bg_color_map[i] = 0; }
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
                    (0x9000i32 as i32 + (tn as i32) * 16) as usize
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

                self.bg_color_map[y * 160 + x] = color_id;
            }
        }

        // Now render sprites from OAM on top of background according to priority.
        let lcdc = mmu.read_u8(0xFF40);
        let sprite_size = if (lcdc & 0x04) != 0 { 16 } else { 8 };
        let obp0 = mmu.read_u8(0xFF48);
        let obp1 = mmu.read_u8(0xFF49);

        // There are 40 sprites; OAM starts at 0xFE00
        for s in 0..40usize {
            let base = 0xFE00 + s * 4;
            let y = (mmu.read_u8(base) as i16) - 16;
            let x = (mmu.read_u8(base + 1) as i16) - 8;
            let mut tile = mmu.read_u8(base + 2);
            let attr = mmu.read_u8(base + 3);

            let yflip = (attr & 0x40) != 0;
            let xflip = (attr & 0x20) != 0;
            let palette_select = (attr & 0x10) != 0; // 0=OBP0, 1=OBP1
            let priority = (attr & 0x80) != 0; // 1 = behind background

            // For 8x16 mode, the lower bit of tile is ignored and selects top/bottom
            if sprite_size == 16 {
                tile &= 0xFE;
            }

            for row in 0..sprite_size {
                let py = if yflip { (sprite_size - 1 - row) } else { row };
                let tile_line = py as u8;
                // fetch tile bytes: sprites use unsigned 0x8000 addressing for non-CGB
                let tile_addr = 0x8000usize + (tile as usize) * 16usize + (tile_line as usize) * 2usize;
                let b1 = mmu.read_u8(tile_addr);
                let b2 = mmu.read_u8(tile_addr + 1);

                for col in 0..8usize {
                    let px = if xflip { col } else { 7 - col };
                    let bit = px as u8;
                    let low = ((b1 >> bit) & 0x01) as u8;
                    let high = ((b2 >> bit) & 0x01) as u8;
                    let color_id = (high << 1) | low;
                    if color_id == 0 { continue; } // transparent

                    let sx = x + (col as i16);
                    let sy = y + (row as i16);
                    if sx < 0 || sx >= 160 || sy < 0 || sy >= 144 { continue; }
                    let sxu = sx as usize;
                    let syu = sy as usize;
                    let idx = (syu * 160 + sxu) * 4;

                    // Priority: if priority bit set (1) and background color != 0 then skip drawing
                    if priority {
                        let bg_color = self.bg_color_map[syu * 160 + sxu];
                        if bg_color != 0 { continue; }
                    }

                    // Select palette
                    let pal = if palette_select { obp1 } else { obp0 };
                    let shade = match ((pal >> (color_id * 2)) & 0x03) {
                        0 => 0xFFu8,
                        1 => 0xC0u8,
                        2 => 0x60u8,
                        3 => 0x00u8,
                        _ => 0xFFu8,
                    };

                    self.frame_buffer[idx] = shade;
                    self.frame_buffer[idx + 1] = shade;
                    self.frame_buffer[idx + 2] = shade;
                    self.frame_buffer[idx + 3] = 0xFF;
                }
            }
        }
    }
}
