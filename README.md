# sim-6502

Very simple 6502 simulator for running llvm-mos-compiled ELF binaries with remote debugging support, based on arv4t example of `gdbstub` project

## Usage

save following test program as `test.c`:
```
#include <stdio.h>

int main() {
    printf("Hello from llvm-mos!\n");
}
```

and compile it with:
```
mos-sim-clang -g test.c -O1
```

It should produce `a.out.elf` binary.

use following .lldbinit file to upload ELF binary to emulator

```
target create a.out.elf

platform select remote-gdb-server
platform connect connect://localhost:9001
platform put-file a.out.elf a.out.elf
platform disconnect

gdb-remote localhost:9001
```

and run `lldb` to start debugging.

You may also need to add following line:
```
settings set target.load-cwd-lldbinit true
```
to enable loading `.lldbinit` from current directory
