use gdbstub::common::Signal;
use gdbstub::target;
use gdbstub::target::ext::base::singlethread::{SingleThreadBase, SingleThreadResume};
use gdbstub::target::{Target, TargetResult};

use crate::emu::{Emu, ExecMode};
use gdbstub_mos_arch::{MOSArch, MosRegs};

use emulator_6502::Interface6502;

// Additional GDB extensions

mod breakpoints;
mod host_io;

impl Target for Emu {
    type Arch = MOSArch;
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
    fn read_registers(&mut self, regs: &mut MosRegs) -> TargetResult<(), Self> {
        regs.pc = self.cpu.get_program_counter();
        regs.a = self.cpu.get_accumulator();
        regs.x = self.cpu.get_x_register();
        regs.y = self.cpu.get_y_register();
        regs.s = self.cpu.get_stack_pointer();
        regs.flags = self.cpu.get_status_register();
        if let Some(im_reg_map) = &self.im_reg_map {
            for (idx, addr) in im_reg_map.iter().cloned().enumerate() {
                regs.rc[idx] = self.system.mem[addr];
            }
        }
        Ok(())
    }

    fn write_registers(&mut self, regs: &MosRegs) -> TargetResult<(), Self> {
        self.cpu.set_program_counter(regs.pc);
        self.cpu.set_accumulator(regs.a);
        self.cpu.set_x_register(regs.x);
        self.cpu.set_y_register(regs.y);
        self.cpu.set_stack_pointer(regs.s);
        self.cpu.set_status_register(regs.flags);

        if let Some(im_reg_map) = &self.im_reg_map {
            for (idx, addr) in im_reg_map.iter().cloned().enumerate() {
                self.system.mem[addr] = regs.rc[idx];
            }
        }

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
        for (addr, val) in (start_addr as usize..).zip(data.iter_mut()) {
            *val = self.system.read(addr as u16);
        }
        Ok(())
    }

    fn write_addrs(&mut self, start_addr: u16, data: &[u8]) -> TargetResult<(), Self> {
        for (addr, val) in (start_addr as usize..).zip(data.iter().copied()) {
            self.system.write(addr as u16, val);
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

        // if signal.is_some() {
        //     return Err("no support for continuing with signal");
        // }

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
