use gdbstub::target;
use gdbstub::target::ext::breakpoints::WatchKind;
use gdbstub::target::TargetResult;

use crate::emu::Emu;
use gdbstub_mos_arch::MosBreakpointKind;

impl target::ext::breakpoints::Breakpoints for Emu {
    #[inline(always)]
    fn support_sw_breakpoint(
        &mut self,
    ) -> Option<target::ext::breakpoints::SwBreakpointOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_hw_watchpoint(
        &mut self,
    ) -> Option<target::ext::breakpoints::HwWatchpointOps<'_, Self>> {
        // Some(self)
        None
    }
}

impl target::ext::breakpoints::SwBreakpoint for Emu {
    fn add_sw_breakpoint(
        &mut self,
        addr: u16,
        _kind: MosBreakpointKind,
    ) -> TargetResult<bool, Self> {
        self.breakpoints.push(addr);
        eprintln!("Add breakpoint {:04x}", addr);
        Ok(true)
    }

    fn remove_sw_breakpoint(
        &mut self,
        addr: u16,
        _kind: MosBreakpointKind,
    ) -> TargetResult<bool, Self> {
        eprintln!("Del breakpoint {:04x}", addr);
        match self.breakpoints.iter().position(|x| *x == addr) {
            None => return Ok(false),
            Some(pos) => {
                self.breakpoints.remove(pos);
            }
        };

        Ok(true)
    }
}

impl target::ext::breakpoints::HwWatchpoint for Emu {
    fn add_hw_watchpoint(
        &mut self,
        addr: u16,
        len: u16,
        kind: WatchKind,
    ) -> TargetResult<bool, Self> {
        for addr in addr..(addr + len) {
            match kind {
                WatchKind::Write => self.watchpoints.push(addr),
                WatchKind::Read => self.watchpoints.push(addr),
                WatchKind::ReadWrite => self.watchpoints.push(addr),
            };
        }

        Ok(true)
    }

    fn remove_hw_watchpoint(
        &mut self,
        addr: u16,
        len: u16,
        kind: WatchKind,
    ) -> TargetResult<bool, Self> {
        for addr in addr..(addr + len) {
            let pos = match self.watchpoints.iter().position(|x| *x == addr) {
                None => return Ok(false),
                Some(pos) => pos,
            };

            match kind {
                WatchKind::Write => self.watchpoints.remove(pos),
                WatchKind::Read => self.watchpoints.remove(pos),
                WatchKind::ReadWrite => self.watchpoints.remove(pos),
            };
        }

        Ok(true)
    }
}
