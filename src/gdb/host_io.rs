use crate::emu::{Emu, InMemoryFile};
use gdbstub::target;
use gdbstub::target::ext::host_io::{
    FsKind, HostIoErrno, HostIoError, HostIoOpenFlags, HostIoOpenMode, HostIoResult,
};

impl target::ext::host_io::HostIo for Emu {
    #[inline(always)]
    fn support_open(&mut self) -> Option<target::ext::host_io::HostIoOpenOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_close(&mut self) -> Option<target::ext::host_io::HostIoCloseOps<'_, Self>> {
        Some(self)
    }

    #[inline(always)]
    fn support_pwrite(&mut self) -> Option<target::ext::host_io::HostIoPwriteOps<'_, Self>> {
        Some(self)
    }
}

impl target::ext::host_io::HostIoOpen for Emu {
    fn open(
        &mut self,
        filename: &[u8],
        _flags: HostIoOpenFlags,
        _mode: HostIoOpenMode,
    ) -> HostIoResult<u32, Self> {
        let new_fd = self.files.keys().min().unwrap_or(&0) + 1;
        let path =
            std::str::from_utf8(filename).map_err(|_| HostIoError::Errno(HostIoErrno::ENOENT))?;

        let file = InMemoryFile::new(path.to_string());
        self.files.insert(new_fd, file);
        Ok(new_fd)
    }
}

impl target::ext::host_io::HostIoClose for Emu {
    fn close(&mut self, fd: u32) -> HostIoResult<(), Self> {
        let file = self.files.get_mut(&fd);
        if let Some(file) = file {
            if &file.data[1..4] == b"ELF" {
                let data = file.data.clone();
                self.load_elf(&data)
                    .map_err(|_| HostIoError::Fatal("Can't parse ELF"))?;
            }
        }
        Ok(())
    }
}

impl target::ext::host_io::HostIoPwrite for Emu {
    fn pwrite(&mut self, fd: u32, _offset: u16, data: &[u8]) -> HostIoResult<u16, Self> {
        let file = self.files.get_mut(&fd);
        if let Some(file) = file {
            file.data.extend(data.iter());
        }
        Ok(data.len() as u16)
    }
}

impl target::ext::host_io::HostIoSetfs for Emu {
    fn setfs(&mut self, _fs: FsKind) -> HostIoResult<(), Self> {
        Ok(())
    }
}
