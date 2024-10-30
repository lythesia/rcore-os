how to debug?

dep: gdb-dashboard, riscv64-unknown-elf-gdb (build from gdb-14.2 target=riscv64-unknown-elf)

`LOG=debug make debug MODE=debug`
- `LOG` enable kernel logging
- `MODE` tell makefile do not use release mode
