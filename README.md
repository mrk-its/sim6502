# sim-6502

Very simple 6502 simulator for running llvm-mos-compiled ELF binaries with remote debugging support, based on arv4t example of `gdbstub` project

## Usage

use following .llvminit file to upload ELF binary to emulator and start debugging:

```
target create a.out
# target modules load -f a.out -s 0

platform select remote-gdb-server
platform connect connect://localhost:9001
platform put-file a.out a.out
platform disconnect

gdb-remote localhost:9001
```
