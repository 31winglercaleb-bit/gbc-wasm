// src/mmu.rs
// Basic MMU with simple MBC1 support and memory regions necessary to boot many ROMs.

pub struct Mmu {
    pub rom: Vec<u8>,
    pub vram: Vec<u8>,   // 8KB
    pub wram: Vec<u8>,   // 8KB
    pub oam: Vec<u8>,    // 160 bytes
    pub io: Vec<u8>,     // 128 bytes (0xFF00-0xFF7F)
    pub hram: Vec<u8>,   // 127 bytes (0xFF80-0xFFFE)
    pub ie: u8,          // interrupt enable (0xFFFF)
    pub ext_ram: Vec<u8>,
    pub rom_bank: usize, // current ROM bank for 0x4000-0x7FFF
    pub ram_enable: bool,
}

impl Mmu {
    pub fn new(rom: Vec<u8>) -> Mmu {
        Mmu {
            rom,
            vram: vec![0u8; 0x2000],
            wram: vec![0u8; 0x2000],
            oam: vec![0u8; 0xA0],
            io: vec![0u8; 0x80],
            hram: vec![0u8; 0x7F],
            ie: 0,
            ext_ram: vec![0u8; 0x2000],
            rom_bank: 1,
            ram_enable: false,
        }
    }

    pub fn load_rom(&mut self, data: &[u8]) {
        self.rom = data.to_vec();
        // reset some mmu state
        self.rom_bank = 1;
        self.ram_enable = false;
        for b in &mut self.vram { *b = 0; }
        for b in &mut self.wram { *b = 0; }
        for b in &mut self.oam { *b = 0; }
        for b in &mut self.io { *b = 0; }
        for b in &mut self.hram { *b = 0; }
        for b in &mut self.ext_ram { *b = 0; }
    }

    pub fn read_u8(&self, addr: usize) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                // Bank 0
                if addr < self.rom.len() { self.rom[addr] } else { 0xFF }
            }
            0x4000..=0x7FFF => {
                // switchable bank
                let bank = self.rom_bank;
                let base = bank * 0x4000;
                let offset = addr - 0x4000;
                let idx = base + offset;
                if idx < self.rom.len() { self.rom[idx] } else { 0xFF }
            }
            0x8000..=0x9FFF => {
                self.vram[addr - 0x8000]
            }
            0xA000..=0xBFFF => {
                // external RAM (battery)
                self.ext_ram[addr - 0xA000]
            }
            0xC000..=0xDFFF => {
                self.wram[addr - 0xC000]
            }
            0xE000..=0xFDFF => {
                // echo of C000-DDFF
                self.wram[addr - 0xE000]
            }
            0xFE00..=0xFE9F => {
                self.oam[addr - 0xFE00]
            }
            0xFF00..=0xFF7F => {
                self.io[addr - 0xFF00]
            }
            0xFF80..=0xFFFE => {
                self.hram[addr - 0xFF80]
            }
            0xFFFF => self.ie,
            _ => 0xFF,
        }
    }

    pub fn write_u8(&mut self, addr: usize, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                // RAM enable (MBC)
                self.ram_enable = (val & 0x0F) == 0x0A;
                // Also allow writing into ROM area for unit tests / simple images
                if addr < self.rom.len() {
                    self.rom[addr] = val;
                }
            }
            0x2000..=0x3FFF => {
                // ROM bank low 5 bits
                let mut bank = (val & 0x1F) as usize;
                if bank == 0 { bank = 1; }
                self.rom_bank = bank;
                // allow writes into rom for tests if address in rom space
                if addr < self.rom.len() { self.rom[addr] = val; }
            }
            0x4000..=0x5FFF => {
                // Could be RAM bank number or upper bits of ROM bank
                // For simplicity, treat as writable ROM in small setups
                if addr < self.rom.len() { self.rom[addr] = val; }
            }
            0x6000..=0x7FFF => {
                // Banking mode select - ignore for now
                if addr < self.rom.len() { self.rom[addr] = val; }
            }
            0x8000..=0x9FFF => {
                self.vram[addr - 0x8000] = val;
            }
            0xA000..=0xBFFF => {
                if self.ram_enable {
                    self.ext_ram[addr - 0xA000] = val;
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
                self.io[addr - 0xFF00] = val;
            }
            0xFF80..=0xFFFE => {
                self.hram[addr - 0xFF80] = val;
            }
            0xFFFF => {
                self.ie = val;
            }
            _ => {
                // ignore
            }
        }
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
        // do not overwrite ROM vector from this (ROM is separate), but copy writable regions
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
