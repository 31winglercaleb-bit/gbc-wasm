use wasm_bindgen::prelude::*;
use console_error_panic_hook;

mod cpu;
use cpu::Cpu;

const WIDTH: usize = 160;
const HEIGHT: usize = 144;
const FB_BYTES: usize = WIDTH * HEIGHT * 4; // RGBA

#[wasm_bindgen]
pub struct Emulator {
    cpu: Cpu,
    rom: Vec<u8>,
    ram: Vec<u8>,
    frame_counter: u32,
}

#[wasm_bindgen]
impl Emulator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Emulator {
        console_error_panic_hook::set_once();
        Emulator {
            cpu: Cpu::new(),
            rom: Vec::new(),
            ram: vec![0; 0x10000],
            frame_counter: 0,
        }
    }

    pub fn load_rom(&mut self, data: &[u8]) {
        self.rom = data.to_vec();
        self.ram.fill(0);
        self.frame_counter = 0;
        // Load first 32KB into 0x0000-0x7FFF as a simple mapper for now
        let copy_len = usize::min(self.rom.len(), 0x8000);
        self.ram[0x0000..0x0000+copy_len].copy_from_slice(&self.rom[..copy_len]);
        self.cpu.reset();
    }

    pub fn step_frame(&mut self) {
        // For now, execute a fixed number of CPU cycles approximating one frame.
        // A Game Boy runs ~4,194,304 Hz; per frame (59.7fps) it's ~70224 cycles.
        const CYCLES_PER_FRAME: u32 = 70224;
        let mut cycles = 0u32;
        while cycles < CYCLES_PER_FRAME {
            let c = self.cpu.step(&mut self.ram);
            cycles = cycles.wrapping_add(c as u32);
        }
        self.frame_counter = self.frame_counter.wrapping_add(1);
    }

    pub fn render_frame(&mut self) -> Vec<u8> {
        let mut fb = vec![0u8; FB_BYTES];
        // Placeholder: use animated pattern influenced by frame_counter
        let t = (self.frame_counter as usize) % 256;
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let i = (y * WIDTH + x) * 4;
                let r = ((x + t) % 256) as u8;
                let g = ((y + t) % 256) as u8;
                let b = ((x + y + t) % 256) as u8;
                fb[i] = r;
                fb[i + 1] = g;
                fb[i + 2] = b;
                fb[i + 3] = 0xFF;
            }
        }
        fb
    }

    pub fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.ram.len());
        out.extend_from_slice(&self.frame_counter.to_le_bytes());
        out.extend_from_slice(&self.cpu.serialize());
        out.extend_from_slice(&self.ram);
        out
    }

    pub fn load_state(&mut self, data: &[u8]) {
        if data.len() < 4 { return; }
        let mut fb_bytes = [0u8;4];
        fb_bytes.copy_from_slice(&data[0..4]);
        self.frame_counter = u32::from_le_bytes(fb_bytes);
        // CPU state is next N bytes (use cpu.deserialize to determine length)
        let cpu_state_size = Cpu::state_size();
        if data.len() < 4 + cpu_state_size { return; }
        let cpu_bytes = &data[4..4+cpu_state_size];
        self.cpu.deserialize(cpu_bytes);
        let ram_data = &data[4+cpu_state_size..];
        let copy_len = usize::min(self.ram.len(), ram_data.len());
        self.ram[..copy_len].copy_from_slice(&ram_data[..copy_len]);
    }

    pub fn screen_width(&self) -> usize { WIDTH }
    pub fn screen_height(&self) -> usize { HEIGHT }
}
