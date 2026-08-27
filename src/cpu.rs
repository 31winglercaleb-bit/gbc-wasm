// src/cpu.rs
// A minimal LR35902 CPU implementation (skeleton) with a small set of opcodes implemented.
// This file is enough to run simple test ROMs that mainly use a few instructions,
// and provides serialization for save states. The full instruction set and timing
// will be expanded over subsequent commits.

#[derive(Clone, Debug)]
pub struct Cpu {
    // 8-bit registers
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    // 16-bit registers
    pub sp: u16,
    pub pc: u16,
}

impl Cpu {
    pub fn new() -> Cpu {
        let mut c = Cpu {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0xFFFE,
            pc: 0x0100,
        };
        c
    }

    pub fn reset(&mut self) {
        // Typical post-boot defaults (approximate). For authentic behavior you can
        // load the GBC boot ROM in the browser UI and the emulator will run it.
        self.a = 0x01;
        self.f = 0xB0;
        self.b = 0x00;
        self.c = 0x13;
        self.d = 0x00;
        self.e = 0xD8;
        self.h = 0x01;
        self.l = 0x4D;
        self.sp = 0xFFFE;
        self.pc = 0x0100;
    }

    pub fn state_size() -> usize {
        // A,F,B,C,D,E,H,L (8) + SP(2) + PC(2)
        12
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(Self::state_size());
        out.push(self.a);
        out.push(self.f);
        out.push(self.b);
        out.push(self.c);
        out.push(self.d);
        out.push(self.e);
        out.push(self.h);
        out.push(self.l);
        out.extend_from_slice(&self.sp.to_le_bytes());
        out.extend_from_slice(&self.pc.to_le_bytes());
        out
    }

    pub fn deserialize(&mut self, data: &[u8]) {
        if data.len() < Self::state_size() { return; }
        self.a = data[0];
        self.f = data[1];
        self.b = data[2];
        self.c = data[3];
        self.d = data[4];
        self.e = data[5];
        self.h = data[6];
        self.l = data[7];
        let sp = u16::from_le_bytes([data[8], data[9]]);
        let pc = u16::from_le_bytes([data[10], data[11]]);
        self.sp = sp;
        self.pc = pc;
    }

    // Execute a single instruction and return cycles consumed (approximate / skeleton)
    // `mem` is the full 65536-byte memory array (MMU must route IO/mirroring etc.).
    pub fn step(&mut self, mem: &mut [u8]) -> u8 {
        let pc = self.pc as usize;
        let opcode = mem[pc];
        // increment PC now; handlers will adjust if they change PC
        self.pc = self.pc.wrapping_add(1);

        match opcode {
            0x00 => { // NOP
                4
            }
            0x3E => { // LD A,d8
                let val = self.read_u8(mem);
                self.a = val;
                8
            }
            0x06 => { // LD B,d8
                let val = self.read_u8(mem);
                self.b = val;
                8
            }
            0x0E => { // LD C,d8
                let val = self.read_u8(mem);
                self.c = val;
                8
            }
            0x16 => { // LD D,d8
                let val = self.read_u8(mem);
                self.d = val;
                8
            }
            0x1E => { // LD E,d8
                let val = self.read_u8(mem);
                self.e = val;
                8
            }
            0x26 => { // LD H,d8
                let val = self.read_u8(mem);
                self.h = val;
                8
            }
            0x2E => { // LD L,d8
                let val = self.read_u8(mem);
                self.l = val;
                8
            }
            0xAF => { // XOR A
                self.a ^= self.a;
                self.f = 0x80; // zero flag set
                4
            }
            0xC3 => { // JP a16
                let addr = self.read_u16(mem);
                self.pc = addr;
                16
            }
            0xCD => { // CALL a16
                let addr = self.read_u16(mem);
                // push PC (return address)
                let ret = self.pc;
                self.sp = self.sp.wrapping_sub(2);
                let sp = self.sp as usize;
                mem[sp] = (ret & 0xFF) as u8;
                mem[sp + 1] = (ret >> 8) as u8;
                self.pc = addr;
                24
            }
            0xC9 => { // RET
                let sp = self.sp as usize;
                let lo = mem[sp];
                let hi = mem[sp + 1];
                let ret = u16::from_le_bytes([lo, hi]);
                self.sp = self.sp.wrapping_add(2);
                self.pc = ret;
                16
            }
            0xFE => { // CP d8 (compare A with immediate)
                let val = self.read_u8(mem);
                let res = self.a.wrapping_sub(val);
                // Set flags: Z if zero, N=1, H if borrow from bit4, C if borrow
                self.f = 0;
                if res == 0 { self.f |= 0x80; }
                self.f |= 0x40; // N
                if (self.a & 0x0F) < (val & 0x0F) { self.f |= 0x20; }
                if self.a < val { self.f |= 0x10; }
                8
            }
            0xE0 => { // LDH (a8),A  (write A to 0xFF00 + a8)
                let off = self.read_u8(mem) as u16;
                let addr = 0xFF00u16.wrapping_add(off);
                mem[addr as usize] = self.a;
                12
            }
            0xE2 => { // LD (C),A  (write A to 0xFF00 + C)
                let addr = 0xFF00u16.wrapping_add(self.c as u16);
                mem[addr as usize] = self.a;
                8
            }
            0xF0 => { // LD A,(a8)
                let off = self.read_u8(mem) as u16;
                let addr = 0xFF00u16.wrapping_add(off);
                self.a = mem[addr as usize];
                12
            }
            0xF2 => { // LD A,(C)
                let addr = 0xFF00u16.wrapping_add(self.c as u16);
                self.a = mem[addr as usize];
                8
            }
            0xEA => { // LD (a16),A
                let addr = self.read_u16(mem);
                mem[addr as usize] = self.a;
                16
            }
            0xFA => { // LD A,(a16)
                let addr = self.read_u16(mem);
                self.a = mem[addr as usize];
                16
            }
            0x32 => { // LD (HL-),A
                let hl = self.get_hl();
                mem[hl as usize] = self.a;
                let hl = hl.wrapping_sub(1);
                self.set_hl(hl);
                8
            }
            0x2A => { // LD A,(HL+)
                let hl = self.get_hl();
                self.a = mem[hl as usize];
                let hl = hl.wrapping_add(1);
                self.set_hl(hl);
                8
            }
            0x76 => { // HALT - stop CPU until interrupt (we'll treat as NOP for now)
                4
            }
            _ => {
                // Unimplemented opcode: log and treat as NOP to avoid lock.
                // In subsequent commits we'll implement the full opcode set.
                // For now we simply return 4 cycles.
                // (Avoid using console logging here to keep the core fast.)
                4
            }
        }
    }

    // Helpers
    fn read_u8(&mut self, mem: &mut [u8]) -> u8 {
        let v = mem[self.pc as usize];
        self.pc = self.pc.wrapping_add(1);
        v
    }

    fn read_u16(&mut self, mem: &mut [u8]) -> u16 {
        let lo = self.read_u8(mem) as u16;
        let hi = self.read_u8(mem) as u16;
        (hi << 8) | lo
    }

    fn get_hl(&self) -> u16 {
        ((self.h as u16) << 8) | (self.l as u16)
    }

    fn set_hl(&mut self, v: u16) {
        self.h = ((v >> 8) & 0xFF) as u8;
        self.l = (v & 0xFF) as u8;
    }
}
