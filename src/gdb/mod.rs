use gdbstub::common::Signal;
use gdbstub::target;
use gdbstub::target::ext::base::singlethread::{SingleThreadBase, SingleThreadResume};
use gdbstub::target::{Target, TargetResult};

use crate::emu::{Emu, ExecMode};
use emulator_6502::Interface6502;

// Additional GDB extensions

mod breakpoints;
mod exec_file;
mod host_io;
mod target_description_xml_override;

/// Copy all bytes of `data` to `buf`.
/// Return the size of data copied.
pub fn copy_to_buf(data: &[u8], buf: &mut [u8]) -> usize {
    let len = buf.len().min(data.len());
    buf[..len].copy_from_slice(&data[..len]);
    len
}

/// Copy a range of `data` (start at `offset` with a size of `length`) to `buf`.
/// Return the size of data copied. Returns 0 if `offset >= buf.len()`.
///
/// Mainly used by qXfer:_object_:read commands.
pub fn copy_range_to_buf(data: &[u8], offset: u64, length: usize, buf: &mut [u8]) -> usize {
    let offset = offset as usize;
    if offset > data.len() {
        return 0;
    }

    let start = offset;
    let end = (offset + length).min(data.len());
    copy_to_buf(&data[start..end], buf)
}

impl Target for Emu {
    // As an example, I've defined a custom architecture based off
    // `gdbstub_arch::arm::Armv4t`. The implementation is in the `custom_arch`
    // module at the bottom of this file.
    //
    // unless you're working with a particularly funky architecture that uses custom
    // registers, you should probably stick to using the simple `target.xml`
    // implementations from the `gdbstub_arch` repo (i.e: `target.xml` files that
    // only specify the <architecture> and <feature>s of the arch, instead of
    // listing out all the registers out manually).
    type Arch = custom_arch::MOSArch;
    type Error = &'static str;

    // --------------- IMPORTANT NOTE ---------------
    // Always remember to annotate IDET enable methods with `inline(always)`!
    // Without this annotation, LLVM might fail to dead-code-eliminate nested IDET
    // implementations, resulting in unnecessary binary bloat.

    #[inline(always)]
    fn base_ops(&mut self) -> target::ext::base::BaseOps<'_, Self::Arch, Self::Error> {
        target::ext::base::BaseOps::SingleThread(self)
    }

    #[inline(always)]
    fn support_breakpoints(
        &mut self,
    ) -> Option<target::ext::breakpoints::BreakpointsOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_register_info(
        &mut self,
    ) -> Option<target::ext::register_info::RegisterInfoOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_target_description_xml_override(
        &mut self,
    ) -> Option<
        target::ext::target_description_xml_override::TargetDescriptionXmlOverrideOps<'_, Self>,
    > {
        Some(self)
    }

    #[inline(always)]
    fn support_host_io(&mut self) -> Option<target::ext::host_io::HostIoOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadBase for Emu {
    fn read_registers(&mut self, regs: &mut custom_arch::MosRegs) -> TargetResult<(), Self> {
        regs.pc = self.cpu.get_program_counter();
        regs.a = self.cpu.get_accumulator();
        regs.x = self.cpu.get_x_register();
        regs.y = self.cpu.get_y_register();
        Ok(())
    }

    fn write_registers(&mut self, regs: &custom_arch::MosRegs) -> TargetResult<(), Self> {
        self.cpu.set_program_counter(regs.pc);
        self.cpu.set_accumulator(regs.a);
        self.cpu.set_x_register(regs.x);
        self.cpu.set_y_register(regs.y);
        Ok(())
    }

    // #[inline(always)]
    // fn support_single_register_access(
    //     &mut self,
    // ) -> Option<target::ext::base::single_register_access::SingleRegisterAccessOps<'_, (), Self>>
    // {
    //     Some(self)
    // }

    fn read_addrs(&mut self, start_addr: u16, data: &mut [u8]) -> TargetResult<(), Self> {
        for (addr, val) in (start_addr..).zip(data.iter_mut()) {
            *val = self.system.read(addr);
        }
        Ok(())
    }

    fn write_addrs(&mut self, start_addr: u16, data: &[u8]) -> TargetResult<(), Self> {
        for (addr, val) in (start_addr..).zip(data.iter().copied()) {
            self.system.write(addr, val);
        }
        Ok(())
    }

    #[inline(always)]
    fn support_resume(
        &mut self,
    ) -> Option<target::ext::base::singlethread::SingleThreadResumeOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadResume for Emu {
    fn resume(&mut self, signal: Option<Signal>) -> Result<(), Self::Error> {
        // Upon returning from the `resume` method, the target being debugged should be
        // configured to run according to whatever resume actions the GDB client has
        // specified (as specified by `set_resume_action`, `resume_range_step`,
        // `reverse_{step, continue}`, etc...)
        //
        // In this basic `armv4t` example, the `resume` method simply sets the exec mode
        // of the emulator's interpreter loop and returns.
        //
        // In more complex implementations, it's likely that the target being debugged
        // will be running in another thread / process, and will require some kind of
        // external "orchestration" to set it's execution mode (e.g: modifying the
        // target's process state via platform specific debugging syscalls).

        if signal.is_some() {
            return Err("no support for continuing with signal");
        }

        self.exec_mode = ExecMode::Continue;

        Ok(())
    }

    #[inline(always)]
    fn support_single_step(
        &mut self,
    ) -> Option<target::ext::base::singlethread::SingleThreadSingleStepOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_range_step(
        &mut self,
    ) -> Option<target::ext::base::singlethread::SingleThreadRangeSteppingOps<'_, Self>> {
        Some(self)
    }
}

impl target::ext::base::singlethread::SingleThreadSingleStep for Emu {
    fn step(&mut self, signal: Option<Signal>) -> Result<(), Self::Error> {
        if signal.is_some() {
            return Err("no support for stepping with signal");
        }

        self.exec_mode = ExecMode::Step;

        Ok(())
    }
}

impl target::ext::base::singlethread::SingleThreadRangeStepping for Emu {
    fn resume_range_step(&mut self, start: u16, end: u16) -> Result<(), Self::Error> {
        self.exec_mode = ExecMode::RangeStep(start, end);
        Ok(())
    }
}

impl target::ext::register_info::RegisterInfo for Emu {
    fn get_register_info(&self, n: usize) -> Option<&'static str> {
        let reg_descr = match n {
            0 => "name:PC;alt-name:pc;bitsize:16;offset:0;encoding:uint;format:hex;set:General Purpose Registers;gcc:16;dwarf:16;generic:pc;",
            1 => "name:A;alt-name:a;bitsize:8;offset:2;encoding:uint;format:hex;set:General Purpose Registers;",
            2 => "name:X;alt-name:x;bitsize:8;offset:3;encoding:uint;format:hex;set:General Purpose Registers;",
            3 => "name:Y;alt-name:y;bitsize:8;offset:4;encoding:uint;format:hex;set:General Purpose Registers;",
            _ => return None
        };
        return Some(reg_descr);
    }
}

pub mod custom_arch {
    use core::num::NonZeroUsize;

    use gdbstub::arch::{Arch, RegId, Registers, SingleStepGdbBehavior};

    /// Implements `Arch` for ARMv4T
    pub enum MOSArch {}

    #[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
    pub struct MosRegs {
        pub pc: u16,
        pub a: u8,
        pub x: u8,
        pub y: u8,
    }

    impl Registers for MosRegs {
        type ProgramCounter = u16;

        fn pc(&self) -> Self::ProgramCounter {
            self.pc
        }

        fn gdb_serialize(&self, mut write_byte: impl FnMut(Option<u8>)) {
            macro_rules! write_bytes {
                ($bytes:expr) => {
                    for b in $bytes {
                        write_byte(Some(*b))
                    }
                };
            }
            write_bytes!(&self.pc.to_le_bytes());
            write_bytes!(&self.a.to_le_bytes());
            write_bytes!(&self.x.to_le_bytes());
            write_bytes!(&self.y.to_le_bytes());
        }

        fn gdb_deserialize(&mut self, bytes: &[u8]) -> Result<(), ()> {
            self.pc = bytes[0] as u16 + bytes[1] as u16 * 256;
            self.a = bytes[2];
            self.x = bytes[3];
            self.y = bytes[4];
            Ok(())
        }
    }

    #[derive(Debug)]
    pub enum MosRegId {
        PC,
        A,
        X,
        Y,
    }

    impl RegId for MosRegId {
        fn from_raw_id(id: usize) -> Option<(Self, Option<NonZeroUsize>)> {
            let (reg, size) = match id {
                0 => (MosRegId::PC, 2),
                1 => (MosRegId::A, 1),
                2 => (MosRegId::X, 1),
                3 => (MosRegId::Y, 1),
                _ => return None,
            };
            return Some((reg, Some(NonZeroUsize::new(size).unwrap())));
        }
    }

    #[derive(Debug)]
    pub enum MosBreakpointKind {
        /// 16-bit Thumb mode breakpoint.
        Regular,
    }

    impl gdbstub::arch::BreakpointKind for MosBreakpointKind {
        fn from_usize(_kind: usize) -> Option<Self> {
            Some(MosBreakpointKind::Regular)
        }
    }

    impl Arch for MOSArch {
        type Usize = u16;
        type Registers = MosRegs;
        type RegId = MosRegId;
        type BreakpointKind = MosBreakpointKind;

        // for _purely demonstrative purposes_, i'll return dummy data from this
        // function, as it will be overwritten by TargetDescriptionXmlOverride.
        //
        // See `examples/armv4t/gdb/target_description_xml_override.rs`
        //
        // in an actual implementation, you'll want to return an actual string here!
        fn target_description_xml() -> Option<&'static str> {
            Some("never gets returned")
        }

        // armv4t supports optional single stepping.
        //
        // notably, x86 is an example of an arch that does _not_ support
        // optional single stepping.
        #[inline(always)]
        fn single_step_gdb_behavior() -> SingleStepGdbBehavior {
            SingleStepGdbBehavior::Optional
        }
    }
}
