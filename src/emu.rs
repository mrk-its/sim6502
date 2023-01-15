use crate::DynResult;
use std::collections::HashMap;

use emulator_6502::{Interface6502, MOS6502};
use goblin::elf::sym::{st_bind, STB_GLOBAL};

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Event {
    DoneStep,
    Halted,
    Break,
    WatchWrite(u16),
    WatchRead(u16),
}

#[derive(Debug)]
pub enum ExecMode {
    Idle,
    Step,
    Continue,
    RangeStep(u16, u16),
}

pub struct InMemoryFile {
    pub filename: String,
    pub data: Vec<u8>,
}

impl InMemoryFile {
    pub fn new(filename: String) -> Self {
        Self {
            filename,
            data: vec![],
        }
    }
}

pub struct System {
    finished: bool,
    cycle_cnt: u64,
    cycle_cnt_save: u64,
    pub mem: [u8; 65536],
}

impl Default for System {
    fn default() -> Self {
        Self {
            finished: false,
            cycle_cnt: 0,
            cycle_cnt_save: 0,
            mem: [0; 65536],
        }
    }
}

impl Interface6502 for System {
    fn read(&mut self, address: u16) -> u8 {
        match address {
            0xfff0 => {
                self.cycle_cnt_save = self.cycle_cnt;
                (self.cycle_cnt_save & 0xff) as u8
            }
            0xfff1 => ((self.cycle_cnt_save >> 8) & 0xff) as u8,
            0xfff2 => ((self.cycle_cnt_save >> 16) & 0xff) as u8,
            0xfff3 => ((self.cycle_cnt_save >> 24) & 0xff) as u8,
            _ => self.mem[address as usize]
        }
    }

    fn write(&mut self, address: u16, data: u8) {
        match address {
            0xfff9 => {
                eprint!("{}", (data & 0x7f) as char);
            }
            0xfff8 => {
                self.finished = true;
            }
            _ => {
                self.mem[address as usize] = data;
            }
        }
    }
}

pub struct Emu {
    pub(crate) exec_mode: ExecMode,
    pub(crate) system: System,
    pub(crate) cpu: MOS6502,
    pub(crate) watchpoints: Vec<u16>,
    pub(crate) breakpoints: Vec<u16>,
    pub(crate) files: HashMap<u32, InMemoryFile>,
    pub(crate) im_reg_map: Option<[usize; 32]>,
}

impl Default for Emu {
    fn default() -> Self {
        Self {
            // regs: Default::default(),
            exec_mode: ExecMode::Idle,
            cpu: MOS6502::new(),
            system: System::default(),
            watchpoints: Default::default(),
            breakpoints: Default::default(),
            files: Default::default(),
            im_reg_map: None,
        }
    }
}

impl Emu {
    pub fn load_elf(&mut self, program_elf: &[u8]) -> DynResult<()> {
        // load ELF
        let elf_header = goblin::elf::Elf::parse(program_elf)?;
        self.im_reg_map = None;
        for sym in elf_header.syms.iter() {
            let sym_name = elf_header.strtab.get_at(sym.st_name).unwrap_or("");
            // println!("HERE: {:?} {}", sym_name, st_bind(sym.st_info) == STB_GLOBAL);
            if sym_name.starts_with("__rc") {
                if let Ok(idx) = sym_name[4..].parse::<usize>() {
                    if idx < 32 && sym.st_value < 256 {
                        let im_reg_map = self.im_reg_map.get_or_insert_with(|| [0; 32]);
                        im_reg_map[idx] = sym.st_value as usize;
                        log::info!("immaginary reg mapping: {} -> {:02x?}", sym_name, sym.st_value);
                    } else {
                        log::warn!("invalid immaginary reg mapping: {} -> {:04x?}", sym_name, sym.st_value);
                    }
                }
            }
        }

        // copy all in-memory sections from the ELF file into system RAM
        let sections = elf_header
            .section_headers
            .iter()
            .filter(|h| h.is_alloc() && h.sh_type != goblin::elf::section_header::SHT_NOBITS);

        self.system = System::default();

        for h in sections {
            eprintln!(
                "loading section {:?} into memory from [{:#010x?}..{:#010x?}]",
                elf_header.shdr_strtab.get_at(h.sh_name).unwrap(),
                h.sh_addr,
                h.sh_addr + h.sh_size,
            );

            for (i, b) in program_elf[h.file_range().unwrap()].iter().enumerate() {
                self.system.write(h.sh_addr as u16 + i as u16, *b);
            }
        }

        self.cpu.set_program_counter(elf_header.entry as u16);
        eprintln!("PC: {:04x}", elf_header.entry as u16);
        self.watchpoints = Default::default();
        self.breakpoints = Default::default();
        self.files = Default::default();
        self.exec_mode = ExecMode::Continue;

        Ok(())
    }

    // pub(crate) fn reset(&mut self) {
    // }

    /// single-step the interpreter
    pub fn step(&mut self) -> Option<Event> {
        // let mut hit_watchpoint = None;

        // let mut sniffer = MemSniffer::new(&mut self.mem, &self.watchpoints, |access| {
        //     hit_watchpoint = Some(access)
        // });

        self.cpu.cycle(&mut self.system);

        self.system.cycle_cnt += 1;
        if self.system.finished {
            self.exec_mode = ExecMode::Idle;
            return Some(Event::Halted);
        }
        let pc = self.cpu.get_program_counter();
        // self.cpu.step(&mut sniffer);
        // let pc = self.cpu.reg_get(Mode::User, reg::PC);

        // if let Some(access) = hit_watchpoint {
        //     let fixup = if self.cpu.thumb_mode() { 2 } else { 4 };
        //     self.cpu.reg_set(Mode::User, reg::PC, pc - fixup);

        //     return Some(match access.kind {
        //         AccessKind::Read => Event::WatchRead(access.addr),
        //         AccessKind::Write => Event::WatchWrite(access.addr),
        //     });
        // }

        if self.breakpoints.contains(&pc) {
            return Some(Event::Break);
        }

        // if pc == HLE_RETURN_ADDR {
        //     return Some(Event::Halted);
        // }

        None
    }

    /// run the emulator in accordance with the currently set `ExecutionMode`.
    ///
    /// since the emulator runs in the same thread as the GDB loop, the emulator
    /// will use the provided callback to poll the connection for incoming data
    /// every 1024 steps.
    pub fn run(&mut self, mut poll_incoming_data: impl FnMut() -> bool) -> RunEvent {
        eprintln!("target run: {:?}", self.exec_mode);
        match self.exec_mode {
            ExecMode::Idle => loop {
                if poll_incoming_data() {
                    break RunEvent::IncomingData;
                }
            },
            ExecMode::Step => RunEvent::Event(self.step().unwrap_or(Event::DoneStep)),
            ExecMode::Continue => {
                let mut cycles = 0;
                loop {
                    if cycles % 1024 == 0 {
                        // poll for incoming data
                        if poll_incoming_data() {
                            break RunEvent::IncomingData;
                        }
                    }
                    cycles += 1;

                    if let Some(event) = self.step() {
                        break RunEvent::Event(event);
                    };
                }
            }
            // just continue, but with an extra PC check
            ExecMode::RangeStep(start, end) => {
                eprintln!("range step");
                let mut cycles = 0;
                loop {
                    if cycles % 1024 == 0 {
                        // poll for incoming data
                        if poll_incoming_data() {
                            break RunEvent::IncomingData;
                        }
                    }
                    cycles += 1;

                    if let Some(event) = self.step() {
                        break RunEvent::Event(event);
                    };

                    if !(start..end).contains(&self.cpu.get_program_counter()) {
                        break RunEvent::Event(Event::DoneStep);
                    }
                }
            }
        }
    }
}

pub enum RunEvent {
    IncomingData,
    Event(Event),
}
