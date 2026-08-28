use wasm_bindgen::prelude::*;
use console_error_panic_hook;

mod cpu;
mod mmu;
mod ppu;

use cpu::Cpu;
use mmu::Mmu;
use ppu::Ppu;

const WIDTH: usize = 160;
const HEIGHT: usize = 144;

#[wasm_bindgen]
pub struct Emulator {
    cpu: Cpu,
    mmu: Mmu,
    ppu: Ppu,
    frame_counter: u32,
}

#[wasm_bindgen]
impl Emulator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Emulator {
        console_error_panic_hook::set_once();
        let mmu = Mmu::new(Vec::new());
        Emulator {
            cpu: Cpu::new(),
            mmu,
            ppu: Ppu::new(),
            frame_counter: 0,
        }
    }

    pub fn load_rom(&mut self, data: &[u8]) {
        self.mmu.load_rom(data);
        self.frame_counter = 0;
        self.cpu.reset();
    }

    pub fn step_frame(&mut self) {
        // Execute CPU cycles approximating one frame.
        const CYCLES_PER_FRAME: u32 = 70224;
        let mut cycles = 0u32;
        while cycles < CYCLES_PER_FRAME {
            let c = self.cpu.step(&mut self.mmu);
            // advance MMU timers
            self.mmu.step(c as u32);
            // advance PPU by cycles
            self.ppu.step(c as u32, &mut self.mmu);
            cycles = cycles.wrapping_add(c as u32);
        }
        self.frame_counter = self.frame_counter.wrapping_add(1);
    }

    pub fn render_frame(&mut self) -> Vec<u8> {
        self.ppu.render_frame()
    }

    pub fn save_state(&self) -> Vec<u8> {
        // frame_counter (4) + cpu state + full memory dump
        let cpu_state = self.cpu.serialize();
        let mem = self.mmu.dump_mem();
        let mut out = Vec::with_capacity(4 + cpu_state.len() + mem.len());
        out.extend_from_slice(&self.frame_counter.to_le_bytes());
        out.extend_from_slice(&cpu_state);
        out.extend_from_slice(&mem);
        out
    }

    pub fn load_state(&mut self, data: &[u8]) {
        if data.len() < 4 { return; }
        let mut fc_bytes = [0u8;4];
        fc_bytes.copy_from_slice(&data[0..4]);
        self.frame_counter = u32::from_le_bytes(fc_bytes);
        let cpu_state_size = Cpu::state_size();
        if data.len() < 4 + cpu_state_size { return; }
        let cpu_bytes = &data[4..4+cpu_state_size];
        self.cpu.deserialize(cpu_bytes);
        let mem_data = &data[4+cpu_state_size..];
        self.mmu.load_from_mem(mem_data);
    }

    pub fn screen_width(&self) -> usize { WIDTH }
    pub fn screen_height(&self) -> usize { HEIGHT }
}
