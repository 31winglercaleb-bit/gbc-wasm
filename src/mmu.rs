// src/mmu.rs
// MMU with improved MBC1 behavior, DMA (OAM) and timer/interrupt stepping.

pub struct Mmu {
    pub rom: Vec<u8>,
    pub vram: Vec<u8>,   // 8KB
    pub wram: Vec<u8>,   // 8KB
    pub oam: Vec<u8>,    // 160 bytes
    pub io: Vec<u8>,     // 128 bytes (0xFF00-0xFF7F)
    pub hram: Vec<u8>,   // 127 bytes (0xFF80-0xFFFE)
    pub ie: u8,          // interrupt enable (0xFFFF)
    pub ext_ram: Vec<u8>,

    // MBC1 registers/state
    rom_bank_lower5: u8,
    rom_bank_high2: u8,
    ram_bank: u8,
    banking_mode: u8,
    ram_enable: bool,

    // runtime derived
    rom_bank: usize,

    // timer/div internal counters
    div_counter: u32,
    tima_counter: u32,
}

impl Mmu {
    pub fn new(rom: Vec<u8>) -> Mmu {
        let mut m = Mmu {
            rom,
            vram: vec![0u8; 0x2000],
            wram: vec![0u8; 0x2000],
            oam: vec![0u8; 0xA0],
            io: vec![0u8; 0x80],
            hram: vec![0u8; 0x7F],
            ie: 0,
            ext_ram: vec![0u8; 0x2000],
            rom_bank_lower5: 1,
            rom_bank_high2: 0,
            ram_bank: 0,
            banking_mode: 0,
            ram_enable: false,
            rom_bank: 1,
            div_counter: 0,
            tima_counter: 0,
        };
        m.recompute_rom_bank();
        m
    }

    fn recompute_rom_bank(&mut self) {
        let lower = (self.rom_bank_lower5 & 0x1F) as usize;
        let upper = (self.rom_bank_high2 & 0x03) as usize;
        if self.banking_mode == 0 {
            // ROM banking mode: upper bits affect ROM bank
            let mut bank = (upper << 5) | lower;
            if bank == 0 { bank = 1; }
            self.rom_bank = bank;
            self.ram_bank = 0;
        } else {
            // RAM banking mode: upper bits select RAM bank, ROM uses lower bits only
            let mut bank = lower;
            if bank == 0 { bank = 1; }
            self.rom_bank = bank;
            self.ram_bank = upper as u8;
        }
    }

    pub fn load_rom(&mut self, data: &[u8]) {
        self.rom = data.to_vec();
        // reset some mmu state
        self.rom_bank_lower5 = 1;
        self.rom_bank_high2 = 0;
        self.ram_bank = 0;
        self.banking_mode = 0;
        self.rom_bank = 1;
        self.ram_enable = false;
        for b in &mut self.vram { *b = 0; }
        for b in &mut self.wram { *b = 0; }
        for b in &mut self.oam { *b = 0; }
        for b in &mut self.io { *b = 0; }
        for b in &mut self.hram { *b = 0; }
        for b in &mut self.ext_ram { *b = 0; }
        self.div_counter = 0;
        self.tima_counter = 0;
    }

    pub fn read_u8(&self, addr: usize) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                if addr < self.rom.len() { self.rom[addr] } else { 0xFF }
            }
            0x4000..=0x7FFF => {
                let bank = self.rom_bank;
                let base = bank * 0x4000;
                let offset = addr - 0x4000;
                let idx = base + offset;
                if idx < self.rom.len() { self.rom[idx] } else { 0xFF }
            }
            0x8000..=0x9FFF => self.vram[addr - 0x8000],
            0xA000..=0xBFFF => self.ext_ram[addr - 0xA000],
            0xC000..=0xDFFF => self.wram[addr - 0xC000],
            0xE000..=0xFDFF => self.wram[addr - 0xE000],
            0xFE00..=0xFE9F => self.oam[addr - 0xFE00],
            0xFF00..=0xFF7F => self.io[addr - 0xFF00],
            0xFF80..=0xFFFE => self.hram[addr - 0xFF80],
            0xFFFF => self.ie,
            _ => 0xFF,
        }
    }

    pub fn write_u8(&mut self, addr: usize, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                // RAM enable (MBC)
                self.ram_enable = (val & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => {
                // ROM bank low 5 bits
                self.rom_bank_lower5 = val & 0x1F;
                if self.rom_bank_lower5 == 0 { self.rom_bank_lower5 = 1; }
                self.recompute_rom_bank();
            }
            0x4000..=0x5FFF => {
                // ROM bank high 2 bits or RAM bank depending on mode
                self.rom_bank_high2 = val & 0x03;
                self.recompute_rom_bank();
            }
            0x6000..=0x7FFF => {
                // banking mode select
                self.banking_mode = val & 0x01;
                self.recompute_rom_bank();
            }
            0x8000..=0x9FFF => {
                self.vram[addr - 0x8000] = val;
            }
            0xA000..=0xBFFF => {
                if self.ram_enable {
                    let ram_index = if self.banking_mode == 1 { (self.ram_bank as usize) * 0x2000 } else { 0 };
                    let off = addr - 0xA000;
                    if ram_index + off < self.ext_ram.len() {
                        self.ext_ram[ram_index + off] = val;
                    }
                }
            }
            0xC000..=0xDFFF => {
                self.wram[addr - 0xC000] = val;
            }
            0xE000..=0xFDFF => {
                self.wram[addr - 0xE000] = val;
            }
            0xFE00..=0xFE9F => {
                self.oam[addr - 0xFE00] = val;
            }
            0xFF00..=0xFF7F => {
                let idx = addr - 0xFF00;
                // DMA trigger at 0xFF46
                if addr == 0xFF46 {
                    // write to IO register as well
                    if idx < self.io.len() { self.io[idx] = val; }
                    let source = (val as usize) << 8;
                    for i in 0..0xA0usize {
                        let b = self.read_u8(source + i);
                        self.oam[i] = b;
                    }
                } else {
                    if idx < self.io.len() { self.io[idx] = val; }
                }
            }
            0xFF80..=0xFFFE => {
                self.hram[addr - 0xFF80] = val;
            }
            0xFFFF => {
                self.ie = val;
            }
            _ => {
                // ignore writes to ROM area
            }
        }
    }

    // Step internal timers based on CPU cycles. This updates DIV/TIMA and triggers timer interrupts.
    pub fn step(&mut self, cycles: u32) {
        // DIV (0xFF04) increments every 256 cycles
        self.div_counter = self.div_counter.wrapping_add(cycles);
        while self.div_counter >= 256 {
            self.div_counter -= 256;
            let div_index = 0x04usize; // io[0x04]
            self.io[div_index] = self.io[div_index].wrapping_add(1);
        }

        // TIMA/TMA/TAC
        let tac = self.io[0x07];
        if tac & 0x04 != 0 {
            // timer enabled
            let freq = match tac & 0x03 {
                0 => 1024u32,
                1 => 16u32,
                2 => 64u32,
                3 => 256u32,
                _ => 1024u32,
            };
            self.tima_counter = self.tima_counter.wrapping_add(cycles);
            while self.tima_counter >= freq {
                self.tima_counter -= freq;
                let tima_idx = 0x05usize;
                let tma_idx = 0x06usize;
                let old = self.io[tima_idx];
                if old == 0xFF {
                    // overflow: reload from TMA and request timer interrupt (bit 2)
                    self.io[tima_idx] = self.io[tma_idx];
                    self.request_interrupt(2);
                } else {
                    self.io[tima_idx] = old.wrapping_add(1);
                }
            }
        }
    }

    fn request_interrupt(&mut self, bit: u8) {
        // IF is at io[0x0F]
        let idx = 0x0Fusize;
        self.io[idx] |= 1u8 << bit;
    }

    // Produce a full 64KB memory image representing current state (ROM + RAM/etc.).
    pub fn dump_mem(&self) -> Vec<u8> {
        let mut mem = vec![0u8; 0x10000];
        for addr in 0..=0xFFFFusize {
            mem[addr] = self.read_u8(addr);
        }
        mem
    }

    // Load mmu internal writable regions from a 64KB memory image.
    pub fn load_from_mem(&mut self, data: &[u8]) {
        if data.len() < 0x10000 { return; }
        // VRAM
        self.vram.copy_from_slice(&data[0x8000..0xA000]);
        // External RAM
        self.ext_ram.copy_from_slice(&data[0xA000..0xC000]);
        // WRAM
        self.wram.copy_from_slice(&data[0xC000..0xE000]);
        // OAM
        self.oam.copy_from_slice(&data[0xFE00..0xFEA0]);
        // IO
        self.io.copy_from_slice(&data[0xFF00..0xFF80]);
        // HRAM
        self.hram.copy_from_slice(&data[0xFF80..0xFFFF]);
        // IE
        self.ie = data[0xFFFF];
        // Note: rom_bank info cannot be recovered easily from raw memory; keep current rom_bank.
    }
}
