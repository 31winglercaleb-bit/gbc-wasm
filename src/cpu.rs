// updated cpu.rs to call mmu read/write helpers

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
        Cpu {
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
        }
    }

    pub fn reset(&mut self) {
        // Typical post-boot defaults (approximate).
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
    // Now uses Mmu read/write helpers instead of raw memory slice.
    pub fn step(&mut self, mmu: &mut crate::mmu::Mmu) -> u8 {
        let pc = self.pc as usize;
        let opcode = mmu.read_u8(pc);
        // increment PC now; handlers will adjust if they change PC
        self.pc = self.pc.wrapping_add(1);

        match opcode {
            0x00 => { // NOP
                4
            }
            // LD r, d8
            0x3E => { // LD A,d8
                let val = self.read_u8(mmu);
                self.a = val;
                8
            }
            0x06 => { // LD B,d8
                let val = self.read_u8(mmu);
                self.b = val;
                8
            }
            0x0E => { // LD C,d8
                let val = self.read_u8(mmu);
                self.c = val;
                8
            }
            0x16 => { // LD D,d8
                let val = self.read_u8(mmu);
                self.d = val;
                8
            }
            0x1E => { // LD E,d8
                let val = self.read_u8(mmu);
                self.e = val;
                8
            }
            0x26 => { // LD H,d8
                let val = self.read_u8(mmu);
                self.h = val;
                8
            }
            0x2E => { // LD L,d8
                let val = self.read_u8(mmu);
                self.l = val;
                8
            }

            // INC r
            0x04 => { // INC B
                let (res, half) = Self::inc8_with_half(self.b);
                self.b = res;
                self.set_flag_z(res);
                self.set_flag_n(false);
                self.set_flag_h(half);
                4
            }
            0x0C => { // INC C
                let (res, half) = Self::inc8_with_half(self.c);
                self.c = res;
                self.set_flag_z(res);
                self.set_flag_n(false);
                self.set_flag_h(half);
                4
            }

            // DEC r
            0x05 => { // DEC B
                let (res, half) = Self::dec8_with_half(self.b);
                self.b = res;
                self.set_flag_z(res);
                self.set_flag_n(true);
                self.set_flag_h(half);
                4
            }
            0x0D => { // DEC C
                let (res, half) = Self::dec8_with_half(self.c);
                self.c = res;
                self.set_flag_z(res);
                self.set_flag_n(true);
                self.set_flag_h(half);
                4
            }

            // ALU ops
            0xAF => { // XOR A
                self.a ^= self.a;
                // Set Z, clear N,H,C
                self.f = 0x80;
                4
            }
            0x80 => { // ADD A,B
                let (res, half, carry) = Self::add8_with_flags(self.a, self.b);
                self.a = res;
                self.set_flag_z(res);
                self.set_flag_n(false);
                self.set_flag_h(half);
                self.set_flag_c(carry);
                4
            }
            0x90 => { // SUB B
                let (res, half, carry) = Self::sub8_with_flags(self.a, self.b);
                self.a = res;
                self.set_flag_z(res);
                self.set_flag_n(true);
                self.set_flag_h(half);
                self.set_flag_c(carry);
                4
            }

            0xC3 => { // JP a16
                let addr = self.read_u16(mmu);
                self.pc = addr;
                16
            }
            0xCD => { // CALL a16
                let addr = self.read_u16(mmu);
                // push PC (return address)
                let ret = self.pc;
                self.sp = self.sp.wrapping_sub(2);
                let sp = self.sp as usize;
                mmu.write_u8(sp, (ret & 0xFF) as u8);
                mmu.write_u8(sp + 1, (ret >> 8) as u8);
                self.pc = addr;
                24
            }
            0xC9 => { // RET
                let sp = self.sp as usize;
                let lo = mmu.read_u8(sp);
                let hi = mmu.read_u8(sp + 1);
                let ret = u16::from_le_bytes([lo, hi]);
                self.sp = self.sp.wrapping_add(2);
                self.pc = ret;
                16
            }
            0xFE => { // CP d8 (compare A with immediate)
                let val = self.read_u8(mmu);
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
                let off = self.read_u8(mmu) as u16;
                let addr = 0xFF00u16.wrapping_add(off) as usize;
                mmu.write_u8(addr, self.a);
                12
            }
            0xE2 => { // LD (C),A  (write A to 0xFF00 + C)
                let addr = 0xFF00u16.wrapping_add(self.c as u16) as usize;
                mmu.write_u8(addr, self.a);
                8
            }
            0xF0 => { // LD A,(a8)
                let off = self.read_u8(mmu) as u16;
                let addr = 0xFF00u16.wrapping_add(off) as usize;
                self.a = mmu.read_u8(addr);
                12
            }
            0xF2 => { // LD A,(C)
                let addr = 0xFF00u16.wrapping_add(self.c as u16) as usize;
                self.a = mmu.read_u8(addr);
                8
            }
            0xEA => { // LD (a16),A
                let addr = self.read_u16(mmu) as usize;
                mmu.write_u8(addr, self.a);
                16
            }
            0xFA => { // LD A,(a16)
                let addr = self.read_u16(mmu) as usize;
                self.a = mmu.read_u8(addr);
                16
            }
            0x32 => { // LD (HL-),A
                let hl = self.get_hl();
                mmu.write_u8(hl as usize, self.a);
                let hl = hl.wrapping_sub(1);
                self.set_hl(hl);
                8
            }
            0x2A => { // LD A,(HL+)
                let hl = self.get_hl();
                self.a = mmu.read_u8(hl as usize);
                let hl = hl.wrapping_add(1);
                self.set_hl(hl);
                8
            }
            0x76 => { // HALT - stop CPU until interrupt (we'll treat as NOP for now)
                4
            }
            _ => {
                // Unimplemented opcode: treat as NOP to avoid lock.
                4
            }
        }
    }

    // Helpers
    fn read_u8(&mut self, mmu: &mut crate::mmu::Mmu) -> u8 {
        let v = mmu.read_u8(self.pc as usize);
        self.pc = self.pc.wrapping_add(1);
        v

    }

    fn read_u16(&mut self, mmu: &mut crate::mmu::Mmu) -> u16 {
        let lo = self.read_u8(mmu) as u16;
        let hi = self.read_u8(mmu) as u16;
        (hi << 8) | lo
    }

    fn get_hl(&self) -> u16 {
        ((self.h as u16) << 8) | (self.l as u16)
    }

    fn set_hl(&mut self, v: u16) {
        self.h = ((v >> 8) & 0xFF) as u8;
        self.l = (v & 0xFF) as u8;
    }

    // Flag helpers (F register bits: Z=7, N=6, H=5, C=4)
    fn set_flag_z(&mut self, v8: u8) {
        if v8 == 0 { self.f |= 0x80; } else { self.f &= !0x80; }
    }
    fn set_flag_n(&mut self, v: bool) {
        if v { self.f |= 0x40; } else { self.f &= !0x40; }
    }
    fn set_flag_h(&mut self, v: bool) {
        if v { self.f |= 0x20; } else { self.f &= !0x20; }
    }
    fn set_flag_c(&mut self, v: bool) {
        if v { self.f |= 0x10; } else { self.f &= !0x10; }
    }

    fn inc8_with_half(v: u8) -> (u8, bool) {
        let res = v.wrapping_add(1);
        let half = ((v & 0x0F) + 1) & 0x10 == 0x10;
        (res, half)
    }

    fn dec8_with_half(v: u8) -> (u8, bool) {
        let res = v.wrapping_sub(1);
        let half = (v & 0x0F) == 0x00; // borrow from bit4 when low nibble was 0
        (res, half)
    }

    fn add8_with_flags(a: u8, b: u8) -> (u8, bool, bool) {
        let (res, carry) = a.overflowing_add(b);
        let half = ((a & 0x0F) + (b & 0x0F)) & 0x10 == 0x10;
        (res, half, carry)
    }

    fn sub8_with_flags(a: u8, b: u8) -> (u8, bool, bool) {
        let (res, borrow) = a.overflowing_sub(b);
        let half = (a & 0x0F) < (b & 0x0F);
        (res, half, borrow)
    }
}

// Unit tests for basic CPU ops updated to use Mmu
#[cfg(test)]
mod tests {
    use super::Cpu;
    use crate::mmu::Mmu;

    fn make_mmu() -> Mmu {
        // Create a ROM-sized vector filled with NOPs (0x00)
        let rom = vec![0x00u8; 0x10000];
        Mmu::new(rom)
    }

    #[test]
    fn test_ld_a_imm() {
        let mut cpu = Cpu::new();
        cpu.pc = 0x0100;
        let mut mmu = make_mmu();
        mmu.write_u8(0x0100, 0x3E); // LD A,d8
        mmu.write_u8(0x0101, 0x42);
        let cycles = cpu.step(&mut mmu);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cycles, 8);
    }

    #[test]
    fn test_inc_b_wrap_and_flags() {
        let mut cpu = Cpu::new();
        cpu.pc = 0x0100;
        cpu.b = 0xFF;
        cpu.f = 0x00;
        let mut mmu = make_mmu();
        mmu.write_u8(0x0100, 0x04); // INC B
        let cycles = cpu.step(&mut mmu);
        assert_eq!(cycles, 4);
        assert_eq!(cpu.b, 0x00);
        // Z and H should be set, N cleared
        assert!(cpu.f & 0x80 != 0); // Z
        assert!(cpu.f & 0x20 != 0); // H
        assert!(cpu.f & 0x40 == 0); // N
    }

    #[test]
    fn test_dec_c_flags() {
        let mut cpu = Cpu::new();
        cpu.pc = 0x0100;
        cpu.c = 0x00;
        cpu.f = 0x00;
        let mut mmu = make_mmu();
        mmu.write_u8(0x0100, 0x0D); // DEC C
        let cycles = cpu.step(&mut mmu);
        assert_eq!(cycles, 4);
        assert_eq!(cpu.c, 0xFF);
        // N and H should be set (half borrow), Z cleared unless result==0
        assert!(cpu.f & 0x40 != 0); // N
        assert!(cpu.f & 0x20 != 0); // H
    }

    #[test]
    fn test_add_a_b() {
        let mut cpu = Cpu::new();
        cpu.pc = 0x0100;
        cpu.a = 0x10;
        cpu.b = 0xF0;
        cpu.f = 0;
        let mut mmu = make_mmu();
        mmu.write_u8(0x0100, 0x80); // ADD A,B
        let cycles = cpu.step(&mut mmu);
        assert_eq!(cycles, 4);
        assert_eq!(cpu.a, 0x00);
        assert!(cpu.f & 0x80 != 0); // Z
        assert!(cpu.f & 0x10 != 0); // C (carry)
    }
}
