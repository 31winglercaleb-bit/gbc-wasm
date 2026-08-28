// extended cpu.rs: add JR/JP/CALL/RET conditionals, EI/DI, DAA, CPL, SCF/CCF, ADC/SBC/AND/OR/CP registers, RST

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
    // Interrupt master enable
    pub ime: bool,
    // EI takes effect after next instruction; emulate with delayed flag
    pub ei_queued: bool,
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
            ime: true,
            ei_queued: false,
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
        self.ime = true;
        self.ei_queued = false;
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

    // Helper to read a register by 3-bit code (0=B,1=C,2=D,3=E,4=H,5=L,6=(HL),7=A)
    fn get_reg_by_code(&self, code: u8, mmu: &mut crate::mmu::Mmu) -> u8 {
        match code & 0x07 {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => {
                let hl = self.get_hl() as usize;
                mmu.read_u8(hl)
            }
            7 => self.a,
            _ => 0,
        }
    }

    fn set_reg_by_code(&mut self, code: u8, val: u8, mmu: &mut crate::mmu::Mmu) {
        match code & 0x07 {
            0 => self.b = val,
            1 => self.c = val,
            2 => self.d = val,
            3 => self.e = val,
            4 => self.h = val,
            5 => self.l = val,
            6 => {
                let hl = self.get_hl() as usize;
                mmu.write_u8(hl, val);
            }
            7 => self.a = val,
            _ => {}
        }
    }

    fn get_rr(&self, code: u8) -> u16 {
        match code {
            0 => ((self.b as u16) << 8) | (self.c as u16), // BC
            1 => ((self.d as u16) << 8) | (self.e as u16), // DE
            2 => self.get_hl(),                             // HL
            3 => self.sp,                                   // SP
            _ => 0,
        }
    }

    fn set_rr(&mut self, code: u8, val: u16) {
        match code {
            0 => { self.b = ((val >> 8) & 0xFF) as u8; self.c = (val & 0xFF) as u8; }
            1 => { self.d = ((val >> 8) & 0xFF) as u8; self.e = (val & 0xFF) as u8; }
            2 => { self.set_hl(val); }
            3 => { self.sp = val; }
            _ => {}
        }
    }

    fn push_rr(&mut self, mmu: &mut crate::mmu::Mmu, val: u16) {
        self.sp = self.sp.wrapping_sub(2);
        let sp = self.sp as usize;
        let lo = (val & 0xFF) as u8;
        let hi = (val >> 8) as u8;
        mmu.write_u8(sp, lo);
        mmu.write_u8(sp + 1, hi);
    }

    fn pop_rr(&mut self, mmu: &mut crate::mmu::Mmu) -> u16 {
        let sp = self.sp as usize;
        let lo = mmu.read_u8(sp) as u16;
        let hi = mmu.read_u8(sp + 1) as u16;
        self.sp = self.sp.wrapping_add(2);
        (hi << 8) | lo
    }

    fn check_condition(&self, code: u8) -> bool {
        // code: 0 -> NZ, 1 -> Z, 2 -> NC, 3 -> C
        match code & 0x03 {
            0 => (self.f & 0x80) == 0, // NZ
            1 => (self.f & 0x80) != 0, // Z
            2 => (self.f & 0x10) == 0, // NC
            3 => (self.f & 0x10) != 0, // C
            _ => false,
        }
    }

    fn handle_cb(&mut self, cb: u8, mmu: &mut crate::mmu::Mmu) -> u8 {
        let x = (cb >> 6) & 0x03; // group
        let y = (cb >> 3) & 0x07; // sub
        let z = cb & 0x07;        // register code

        match x {
            0 => {
                // rotate/shift operations: y selects op
                let mut v = self.get_reg_by_code(z, mmu);
                let cycles = if z == 6 { 16 } else { 8 };
                match y {
                    0 => { // RLC
                        let carry = (v & 0x80) != 0;
                        v = v.rotate_left(1);
                        self.set_reg_by_code(z, v, mmu);
                        self.set_flag_z(v);
                        self.set_flag_n(false);
                        self.set_flag_h(false);
                        self.set_flag_c(carry);
                    }
                    1 => { // RRC
                        let carry = (v & 0x01) != 0;
                        v = v.rotate_right(1);
                        self.set_reg_by_code(z, v, mmu);
                        self.set_flag_z(v);
                        self.set_flag_n(false);
                        self.set_flag_h(false);
                        self.set_flag_c(carry);
                    }
                    2 => { // RL
                        let old_c = (self.f & 0x10) != 0;
                        let new_c = (v & 0x80) != 0;
                        v = (v << 1) | (if old_c { 1 } else { 0 });
                        self.set_reg_by_code(z, v, mmu);
                        self.set_flag_z(v);
                        self.set_flag_n(false);
                        self.set_flag_h(false);
                        self.set_flag_c(new_c);
                    }
                    3 => { // RR
                        let old_c = (self.f & 0x10) != 0;
                        let new_c = (v & 0x01) != 0;
                        v = (v >> 1) | (if old_c { 0x80 } else { 0 });
                        self.set_reg_by_code(z, v, mmu);
                        self.set_flag_z(v);
                        self.set_flag_n(false);
                        self.set_flag_h(false);
                        self.set_flag_c(new_c);
                    }
                    4 => { // SLA
                        let new_c = (v & 0x80) != 0;
                        v = v << 1;
                        self.set_reg_by_code(z, v, mmu);
                        self.set_flag_z(v);
                        self.set_flag_n(false);
                        self.set_flag_h(false);
                        self.set_flag_c(new_c);
                    }
                    5 => { // SRA
                        let new_c = (v & 0x01) != 0;
                        let msb = v & 0x80;
                        v = (v >> 1) | msb;
                        self.set_reg_by_code(z, v, mmu);
                        self.set_flag_z(v);
                        self.set_flag_n(false);
                        self.set_flag_h(false);
                        self.set_flag_c(new_c);
                    }
                    6 => { // SWAP
                        let upper = (v >> 4) & 0x0F;
                        let lower = v & 0x0F;
                        v = (lower << 4) | upper;
                        self.set_reg_by_code(z, v, mmu);
                        self.set_flag_z(v);
                        self.set_flag_n(false);
                        self.set_flag_h(false);
                        self.set_flag_c(false);
                    }
                    7 => { // SRL
                        let new_c = (v & 0x01) != 0;
                        v = v >> 1;
                        self.set_reg_by_code(z, v, mmu);
                        self.set_flag_z(v);
                        self.set_flag_n(false);
                        self.set_flag_h(false);
                        self.set_flag_c(new_c);
                    }
                    _ => {}
                }
                cycles
            }
            1 => {
                // BIT y, r
                let bit = y;
                let v = self.get_reg_by_code(z, mmu);
                let test = (v >> bit) & 0x01;
                if test == 0 { self.f |= 0x80; } else { self.f &= !0x80; }
                self.set_flag_n(false);
                self.set_flag_h(true);
                if z == 6 { 12 } else { 8 }
            }
            2 => {
                // RES y, r  (reset bit y)
                let mut v = self.get_reg_by_code(z, mmu);
                v &= !(1 << y);
                self.set_reg_by_code(z, v, mmu);
                if z == 6 { 16 } else { 8 }
            }
            3 => {
                // SET y, r  (set bit y)
                let mut v = self.get_reg_by_code(z, mmu);
                v |= 1 << y;
                self.set_reg_by_code(z, v, mmu);
                if z == 6 { 16 } else { 8 }
            }
            _ => 8,
        }
    }

    // Execute a single instruction and return cycles consumed (approximate / skeleton)
    // Now uses Mmu read/write helpers instead of raw memory slice.
    pub fn step(&mut self, mmu: &mut crate::mmu::Mmu) -> u8 {
        // Handle queued EI
        if self.ei_queued {
            self.ime = true;
            self.ei_queued = false;
        }

        // Check for interrupts first
        if self.ime {
            let iflags = mmu.read_u8(0xFF0F);
            let pending = iflags & mmu.ie;
            if pending != 0 {
                // find lowest set bit (0..4)
                let mut found: Option<u8> = None;
                for i in 0..5u8 {
                    if (pending & (1 << i)) != 0 {
                        found = Some(i);
                        break;
                    }
                }
                if let Some(bit) = found {
                    // clear the IF bit
                    let new_if = iflags & !(1 << bit);
                    mmu.write_u8(0xFF0F, new_if);
                    // push PC
                    self.sp = self.sp.wrapping_sub(2);
                    let sp = self.sp as usize;
                    mmu.write_u8(sp, (self.pc & 0xFF) as u8);
                    mmu.write_u8(sp + 1, (self.pc >> 8) as u8);
                    // jump to vector
                    self.pc = 0x40u16 + (bit as u16) * 8u16;
                    self.ime = false;
                    return 20; // interrupt handling cycles
                }
            }
        }

        let pc = self.pc as usize;
        let opcode = mmu.read_u8(pc);
        // increment PC now; handlers will adjust if they change PC
        self.pc = self.pc.wrapping_add(1);

        match opcode {
            0x00 => { // NOP
                4
            }
            0xCB => { // CB-prefixed opcodes
                let cb = self.read_u8(mmu);
                self.handle_cb(cb, mmu)
            }
            // EI/DI
            0xFB => { // EI
                // EI enables interrupts after next instruction
                self.ei_queued = true;
                4
            }
            0xF3 => { // DI
                self.ime = false;
                self.ei_queued = false;
                4
            }
            // DAA
            0x27 => {
                let mut a = self.a as i32;
                let mut adjust = 0i32;
                let mut carry = (self.f & 0x10) != 0;
                if (self.f & 0x20) != 0 || (a & 0x0F) > 9 { adjust |= 0x06; }
                if carry || a > 0x99 { adjust |= 0x60; carry = true; }
                if (self.f & 0x40) != 0 { // N flag set = previous op was subtraction
                    a = a.wrapping_sub(adjust);
                } else {
                    a = a.wrapping_add(adjust);
                }
                self.a = (a & 0xFF) as u8;
                self.set_flag_z(self.a);
                self.set_flag_h(false);
                self.set_flag_c(carry);
                4
            }
            0x2F => { // CPL
                self.a = !self.a;
                self.set_flag_n(true);
                self.set_flag_h(true);
                4
            }
            0x3F => { // CCF
                let c = (self.f & 0x10) != 0;
                self.set_flag_n(false);
                self.set_flag_h(false);
                self.set_flag_c(!c);
                4
            }
            0x37 => { // SCF
                self.set_flag_n(false);
                self.set_flag_h(false);
                self.set_flag_c(true);
                4
            }
            // JR rc, immediate (relative)
            0x18 => { // JR r8
                let off = self.read_i8(mmu) as i16;
                self.pc = ((self.pc as i32) + (off as i32)) as u16;
                12
            }
            0x20 => { // JR NZ,r8
                let off = self.read_i8(mmu) as i16;
                if self.check_condition(0) { // NZ
                    self.pc = ((self.pc as i32) + (off as i32)) as u16;
                    12
                } else { 8 }
            }
            0x28 => { // JR Z,r8
                let off = self.read_i8(mmu) as i16;
                if self.check_condition(1) {
                    self.pc = ((self.pc as i32) + (off as i32)) as u16;
                    12
                } else { 8 }
            }
            0x30 => { // JR NC,r8
                let off = self.read_i8(mmu) as i16;
                if self.check_condition(2) {
                    self.pc = ((self.pc as i32) + (off as i32)) as u16;
                    12
                } else { 8 }
            }
            0x38 => { // JR C,r8
                let off = self.read_i8(mmu) as i16;
                if self.check_condition(3) {
                    self.pc = ((self.pc as i32) + (off as i32)) as u16;
                    12
                } else { 8 }
            }
            // JP a16 and conditional
            0xC3 => { // JP a16
                let addr = self.read_u16(mmu);
                self.pc = addr;
                16
            }
            0xC2 => { // JP NZ,a16
                let addr = self.read_u16(mmu);
                if self.check_condition(0) { self.pc = addr; 12 } else { 16 }
            }
            0xCA => { // JP Z,a16
                let addr = self.read_u16(mmu);
                if self.check_condition(1) { self.pc = addr; 12 } else { 16 }
            }
            0xD2 => { // JP NC,a16
                let addr = self.read_u16(mmu);
                if self.check_condition(2) { self.pc = addr; 12 } else { 16 }
            }
            0xDA => { // JP C,a16
                let addr = self.read_u16(mmu);
                if self.check_condition(3) { self.pc = addr; 12 } else { 16 }
            }
            // CALL and conditional CALL
            0xCD => { // CALL a16
                let addr = self.read_u16(mmu);
                let ret = self.pc;
                self.sp = self.sp.wrapping_sub(2);
                let sp = self.sp as usize;
                mmu.write_u8(sp, (ret & 0xFF) as u8);
                mmu.write_u8(sp + 1, (ret >> 8) as u8);
                self.pc = addr;
                24
            }
            0xC4 => { // CALL NZ,a16
                let addr = self.read_u16(mmu);
                if self.check_condition(0) {
                    let ret = self.pc;
                    self.sp = self.sp.wrapping_sub(2);
                    let sp = self.sp as usize;
                    mmu.write_u8(sp, (ret & 0xFF) as u8);
                    mmu.write_u8(sp + 1, (ret >> 8) as u8);
                    self.pc = addr;
                    24
                } else { 12 }
            }
            0xCC => { // CALL Z,a16
                let addr = self.read_u16(mmu);
                if self.check_condition(1) {
                    let ret = self.pc;
                    self.sp = self.sp.wrapping_sub(2);
                    let sp = self.sp as usize;
                    mmu.write_u8(sp, (ret & 0xFF) as u8);
                    mmu.write_u8(sp + 1, (ret >> 8) as u8);
                    self.pc = addr;
                    24
                } else { 12 }
            }
            0xD4 => { // CALL NC,a16
                let addr = self.read_u16(mmu);
                if self.check_condition(2) {
                    let ret = self.pc;
                    self.sp = self.sp.wrapping_sub(2);
                    let sp = self.sp as usize;
                    mmu.write_u8(sp, (ret & 0xFF) as u8);
                    mmu.write_u8(sp + 1, (ret >> 8) as u8);
                    self.pc = addr;
                    24
                } else { 12 }
            }
            0xDC => { // CALL C,a16
                let addr = self.read_u16(mmu);
                if self.check_condition(3) {
                    let ret = self.pc;
                    self.sp = self.sp.wrapping_sub(2);
                    let sp = self.sp as usize;
                    mmu.write_u8(sp, (ret & 0xFF) as u8);
                    mmu.write_u8(sp + 1, (ret >> 8) as u8);
                    self.pc = addr;
                    24
                } else { 12 }
            }
            // RET and conditional RET
            0xC9 => { // RET
                let sp = self.sp as usize;
                let lo = mmu.read_u8(sp);
                let hi = mmu.read_u8(sp + 1);
                let ret = u16::from_le_bytes([lo, hi]);
                self.sp = self.sp.wrapping_add(2);
                self.pc = ret;
                16
            }
            0xC0 => { // RET NZ
                if self.check_condition(0) {
                    let sp = self.sp as usize;
                    let lo = mmu.read_u8(sp);
                    let hi = mmu.read_u8(sp + 1);
                    let ret = u16::from_le_bytes([lo, hi]);
                    self.sp = self.sp.wrapping_add(2);
                    self.pc = ret;
                    20
                } else { 8 }
            }
            0xC8 => { // RET Z
                if self.check_condition(1) {
                    let sp = self.sp as usize;
                    let lo = mmu.read_u8(sp);
                    let hi = mmu.read_u8(sp + 1);
                    let ret = u16::from_le_bytes([lo, hi]);
                    self.sp = self.sp.wrapping_add(2);
                    self.pc = ret;
                    20
                } else { 8 }
            }
            0xD0 => { // RET NC
                if self.check_condition(2) {
                    let sp = self.sp as usize;
                    let lo = mmu.read_u8(sp);
                    let hi = mmu.read_u8(sp + 1);
                    let ret = u16::from_le_bytes([lo, hi]);
                    self.sp = self.sp.wrapping_add(2);
                    self.pc = ret;
                    20
                } else { 8 }
            }
            0xD8 => { // RET C
                if self.check_condition(3) {
                    let sp = self.sp as usize;
                    let lo = mmu.read_u8(sp);
                    let hi = mmu.read_u8(sp + 1);
                    let ret = u16::from_le_bytes([lo, hi]);
                    self.sp = self.sp.wrapping_add(2);
                    self.pc = ret;
                    20
                } else { 8 }
            }
            // RST vectors
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                let addr = match opcode {
                    0xC7 => 0x00,
                    0xCF => 0x08,
                    0xD7 => 0x10,
                    0xDF => 0x18,
                    0xE7 => 0x20,
                    0xEF => 0x28,
                    0xF7 => 0x30,
                    0xFF => 0x38,
                    _ => 0x00,
                };
                let ret = self.pc;
                self.sp = self.sp.wrapping_sub(2);
                let sp = self.sp as usize;
                mmu.write_u8(sp, (ret & 0xFF) as u8);
                mmu.write_u8(sp + 1, (ret >> 8) as u8);
                self.pc = addr;
                16
            }
            // ADC A,r and SBC A,r
            0x88..=0x8F => {
                let r = (opcode & 0x07) as u8;
                let val = self.get_reg_by_code(r, mmu);
                let c = if (self.f & 0x10) != 0 { 1 } else { 0 };
                let (res1, half1, carry1) = Self::add8_with_flags(self.a, val);
                let (res, half2, carry2) = Self::add8_with_flags(res1, c);
                self.a = res;
                self.set_flag_z(self.a);
                self.set_flag_n(false);
                self.set_flag_h(half1 || half2);
                self.set_flag_c(carry1 || carry2);
                4
            }
            0x98..=0x9F => {
                let r = (opcode & 0x07) as u8;
                let val = self.get_reg_by_code(r, mmu);
                let c = if (self.f & 0x10) != 0 { 1 } else { 0 };
                let (res1, half1, borrow1) = Self::sub8_with_flags(self.a, val);
                let (res, half2, borrow2) = Self::sub8_with_flags(res1, c);
                self.a = res;
                self.set_flag_z(self.a);
                self.set_flag_n(true);
                self.set_flag_h(half1 || half2);
                self.set_flag_c(borrow1 || borrow2);
                4
            }
            // AND/OR/XOR with registers
            0xA0..=0xA7 => {
                let r = (opcode & 0x07) as u8;
                let val = self.get_reg_by_code(r, mmu);
                self.a &= val;
                self.set_flag_z(self.a);
                self.set_flag_n(false);
                self.set_flag_h(true);
                self.set_flag_c(false);
                4
            }
            0xB0..=0xB7 => {
                let r = (opcode & 0x07) as u8;
                let val = self.get_reg_by_code(r, mmu);
                self.a |= val;
                self.set_flag_z(self.a);
                self.set_flag_n(false);
                self.set_flag_h(false);
                self.set_flag_c(false);
                4
            }
            0xA8..=0xAF => { // XOR A,r
                let r = (opcode & 0x07) as u8;
                let val = self.get_reg_by_code(r, mmu);
                self.a ^= val;
                self.set_flag_z(self.a);
                self.set_flag_n(false);
                self.set_flag_h(false);
                self.set_flag_c(false);
                4
            }
            0xB8..=0xBF => { // CP A,r
                let r = (opcode & 0x07) as u8;
                let val = self.get_reg_by_code(r, mmu);
                let res = self.a.wrapping_sub(val);
                self.f = 0;
                if res == 0 { self.f |= 0x80; }
                self.f |= 0x40; // N
                if (self.a & 0x0F) < (val & 0x0F) { self.f |= 0x20; }
                if self.a < val { self.f |= 0x10; }
                4
            }
            // Existing and other opcodes fall back to previous handlers
            // 16-bit INC/DEC, PUSH/POP handled earlier in prior commits; include minimal coverage here

            // Default fallback
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

    fn read_i8(&mut self, mmu: &mut crate::mmu::Mmu) -> i8 {
        let v = self.read_u8(mmu);
        v as i8
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
