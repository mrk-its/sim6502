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
    fn support_host_io(&mut self) -> Option<target::ext::host_io::HostIoOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadBase for Emu {
    fn read_registers(&mut self, regs: &mut custom_arch::MosRegs) -> TargetResult<(), Self> {
        regs.rc.copy_from_slice(&self.system.mem[0..32]);
        regs.rs.iter_mut().enumerate().for_each(|(i, v)| *v = self.system.mem[i * 2] as u16 + self.system.mem[i * 2 + 1] as u16 * 256);
        regs.pc = self.cpu.get_program_counter();
        regs.a = self.cpu.get_accumulator();
        regs.x = self.cpu.get_x_register();
        regs.y = self.cpu.get_y_register();
        regs.s = self.cpu.get_stack_pointer();
        regs.flags = self.cpu.get_status_register();
        Ok(())
    }

    fn write_registers(&mut self, regs: &custom_arch::MosRegs) -> TargetResult<(), Self> {
        self.cpu.set_program_counter(regs.pc);
        self.cpu.set_accumulator(regs.a);
        self.cpu.set_x_register(regs.x);
        self.cpu.set_y_register(regs.y);
        self.cpu.set_stack_pointer(regs.s);
        self.cpu.set_status_register(regs.flags);
        self.system.mem[0..32].copy_from_slice(&regs.rc);

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

pub mod custom_arch {
    use core::num::NonZeroUsize;

    use gdbstub::arch::{Arch, RegId, Registers, SingleStepGdbBehavior};

    /// Implements `Arch` for ARMv4T
    pub enum MOSArch {}

    #[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
    pub struct MosRegs {
        pub rc: [u8; 32],
        pub rs: [u16; 16],
        pub pc: u16,
        pub a: u8,
        pub x: u8,
        pub y: u8,
        pub s: u8,
        pub flags: u8,
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
            write_bytes!(&self.s.to_le_bytes());
            write_bytes!(&(self.flags & 1).to_le_bytes());
            write_bytes!(&((self.flags >> 1) & 1).to_le_bytes());
            write_bytes!(&((self.flags >> 6) & 1).to_le_bytes());
            write_bytes!(&((self.flags >> 7) & 1).to_le_bytes());

            self.rc.iter().for_each(|v| write_byte(Some(*v)));
            self.rs.iter().for_each(|v| write_bytes!(&v.to_le_bytes()));
        }

        fn gdb_deserialize(&mut self, bytes: &[u8]) -> Result<(), ()> {
            self.pc = bytes[0] as u16 + bytes[1] as u16 * 256;
            self.a = bytes[2];
            self.x = bytes[3];
            self.y = bytes[4];
            self.s = bytes[5];

            self.flags &= 0b00111100;
            self.flags |= bytes[6] | bytes[7] * 2 | bytes[8] * 64 + bytes[9] * 128;

            self.rc.iter_mut().enumerate().for_each(|(i, v)| *v = bytes[10 + i]);
            self.rs.iter_mut().enumerate().for_each(
                |(i, v)| *v = bytes[10 + 32 + i * 2] as u16 + bytes[10 + 32 + i * 2 + 1] as u16 * 256
            );
            Ok(())
        }
    }

    #[derive(Debug)]
    pub enum MosRegId {
        RC(usize),
        RS(usize),
        PC,
        A,
        X,
        Y,
        S,
        C,
        Z,
        N,
        V,
    }

    impl RegId for MosRegId {
        fn from_raw_id(id: usize) -> Option<(Self, Option<NonZeroUsize>)> {
            let (reg, size) = match id {
                0 => (MosRegId::PC, 2),
                1 => (MosRegId::A, 1),
                2 => (MosRegId::X, 1),
                3 => (MosRegId::Y, 1),
                4 => (MosRegId::S, 1),
                5 => (MosRegId::C, 1),
                6 => (MosRegId::Z, 1),
                7 => (MosRegId::N, 1),
                8 => (MosRegId::V, 1),
                9..=40 => (MosRegId::RC(id-9), 1),
                41..=56 => (MosRegId::RS(id-41), 2),
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

        fn target_description_xml() -> Option<&'static str> {

            Some(r#"
            <?xml version="1.0"?>
            <!DOCTYPE target SYSTEM "gdb-target.dtd">
            <target version="1.0">
                <architecture>mos</architecture>
                <flags id="flags" size="1">
                    <field name="C" start="0" end="0" type="bool" />
                    <field name="Z" start="1" end="1" type="bool" />
                    <field name="V" start="6" end="6" type="bool" />
                    <field name="N" start="7" end="7" type="bool" />
                </flags>
                <groups>
                    <group id="1" name="imaginary, 8-bit"></group>
                    <group id="2" name="imaginary, 16-bit"></group>
                </groups>
                <feature name="org.gnu.gdb.mos">
                    <reg name="PC" bitsize="16" offset="0" regnum="0" generic="pc" />
                    <reg name="A" bitsize="8" offset="2" regnum="1" dwarf_regnum="0" />
                    <reg name="X" bitsize="8" offset="3" regnum="2" dwarf_regnum="2" />
                    <reg name="Y" bitsize="8" offset="4" regnum="3" dwarf_regnum="4" />
                    <reg name="S" bitsize="8" offset="5" regnum="4" />
                    <reg name="C" bitsize="1" offset="6" regnum="5" />
                    <reg name="Z" bitsize="1" offset="7" regnum="6" />
                    <reg name="V" bitsize="1" offset="8" regnum="7" />
                    <reg name="N" bitsize="1" offset="9" regnum="8" />
                    <reg name="RC0" group_id="1" bitsize="8" offset="10" regnum="9" dwarf_regnum="16" />
                    <reg name="RC1" group_id="1" bitsize="8" offset="11" regnum="10" dwarf_regnum="18" />
                    <reg name="RC2" group_id="1" bitsize="8" offset="12" regnum="11" dwarf_regnum="20" />
                    <reg name="RC3" group_id="1" bitsize="8" offset="13" regnum="12" dwarf_regnum="22" />
                    <reg name="RC4" group_id="1" bitsize="8" offset="14" regnum="13" dwarf_regnum="24" />
                    <reg name="RC5" group_id="1" bitsize="8" offset="15" regnum="14" dwarf_regnum="26" />
                    <reg name="RC6" group_id="1" bitsize="8" offset="16" regnum="15" dwarf_regnum="28" />
                    <reg name="RC7" group_id="1" bitsize="8" offset="17" regnum="16" dwarf_regnum="30" />
                    <reg name="RC8" group_id="1" bitsize="8" offset="18" regnum="17" dwarf_regnum="32" />
                    <reg name="RC9" group_id="1" bitsize="8" offset="19" regnum="18" dwarf_regnum="34" />
                    <reg name="RC10" group_id="1" bitsize="8" offset="20" regnum="19" dwarf_regnum="36" />
                    <reg name="RC11" group_id="1" bitsize="8" offset="21" regnum="20" dwarf_regnum="38" />
                    <reg name="RC12" group_id="1" bitsize="8" offset="22" regnum="21" dwarf_regnum="40" />
                    <reg name="RC13" group_id="1" bitsize="8" offset="23" regnum="22" dwarf_regnum="42" />
                    <reg name="RC14" group_id="1" bitsize="8" offset="24" regnum="23" dwarf_regnum="44" />
                    <reg name="RC15" group_id="1" bitsize="8" offset="25" regnum="24" dwarf_regnum="46" />
                    <reg name="RC16" group_id="1" bitsize="8" offset="26" regnum="25" dwarf_regnum="48" />
                    <reg name="RC17" group_id="1" bitsize="8" offset="27" regnum="26" dwarf_regnum="50" />
                    <reg name="RC18" group_id="1" bitsize="8" offset="28" regnum="27" dwarf_regnum="52" />
                    <reg name="RC19" group_id="1" bitsize="8" offset="29" regnum="28" dwarf_regnum="54" />
                    <reg name="RC20" group_id="1" bitsize="8" offset="30" regnum="29" dwarf_regnum="56" />
                    <reg name="RC21" group_id="1" bitsize="8" offset="31" regnum="30" dwarf_regnum="58" />
                    <reg name="RC22" group_id="1" bitsize="8" offset="32" regnum="31" dwarf_regnum="60" />
                    <reg name="RC23" group_id="1" bitsize="8" offset="33" regnum="32" dwarf_regnum="62" />
                    <reg name="RC24" group_id="1" bitsize="8" offset="34" regnum="33" dwarf_regnum="64" />
                    <reg name="RC25" group_id="1" bitsize="8" offset="35" regnum="34" dwarf_regnum="66" />
                    <reg name="RC26" group_id="1" bitsize="8" offset="36" regnum="35" dwarf_regnum="68" />
                    <reg name="RC27" group_id="1" bitsize="8" offset="37" regnum="36" dwarf_regnum="70" />
                    <reg name="RC28" group_id="1" bitsize="8" offset="38" regnum="37" dwarf_regnum="72" />
                    <reg name="RC29" group_id="1" bitsize="8" offset="39" regnum="38" dwarf_regnum="74" />
                    <reg name="RC30" group_id="1" bitsize="8" offset="40" regnum="39" dwarf_regnum="76" />
                    <reg name="RC31" group_id="1" bitsize="8" offset="41" regnum="40" dwarf_regnum="78" />
                    <reg name="RS0" group_id="2" bitsize="16" offset="42" regnum="41" dwarf_regnum="528" />
                    <reg name="RS1" group_id="2" bitsize="16" offset="44" regnum="42" dwarf_regnum="529" />
                    <reg name="RS2" group_id="2" bitsize="16" offset="46" regnum="43" dwarf_regnum="530" />
                    <reg name="RS3" group_id="2" bitsize="16" offset="48" regnum="44" dwarf_regnum="531" />
                    <reg name="RS4" group_id="2" bitsize="16" offset="50" regnum="45" dwarf_regnum="532" />
                    <reg name="RS5" group_id="2" bitsize="16" offset="52" regnum="46" dwarf_regnum="533" />
                    <reg name="RS6" group_id="2" bitsize="16" offset="54" regnum="47" dwarf_regnum="534" />
                    <reg name="RS7" group_id="2" bitsize="16" offset="56" regnum="48" dwarf_regnum="535" />
                    <reg name="RS8" group_id="2" bitsize="16" offset="58" regnum="49" dwarf_regnum="536" />
                    <reg name="RS9" group_id="2" bitsize="16" offset="60" regnum="50" dwarf_regnum="537" />
                    <reg name="RS10" group_id="2" bitsize="16" offset="62" regnum="51" dwarf_regnum="538" />
                    <reg name="RS11" group_id="2" bitsize="16" offset="64" regnum="52" dwarf_regnum="539" />
                    <reg name="RS12" group_id="2" bitsize="16" offset="66" regnum="53" dwarf_regnum="540" />
                    <reg name="RS13" group_id="2" bitsize="16" offset="68" regnum="54" dwarf_regnum="541" />
                    <reg name="RS14" group_id="2" bitsize="16" offset="70" regnum="55" dwarf_regnum="542" />
                    <reg name="RS15" group_id="2" bitsize="16" offset="72" regnum="56" dwarf_regnum="543" />
                </feature>
            </target>
            "#)
        }

        #[inline(always)]
        fn single_step_gdb_behavior() -> SingleStepGdbBehavior {
            SingleStepGdbBehavior::Optional
        }
    }
}
