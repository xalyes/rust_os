cargo build -Zbuild-std=core --target x86_64-default_settings.json --release^
 && xcopy /y target\x86_64-default_settings\release\rust_os build\kernel^
 && cargo build --package loader --target x86_64-unknown-uefi -Z build-std=core -Z build-std-features=compiler-builtins-mem^
 && cargo +stable run --package disk_image --target x86_64-pc-windows-msvc  -- target/x86_64-unknown-uefi/debug/loader.efi target/x86_64-default_settings/release/rust_os^
 && qemu-system-x86_64 -drive format=raw,file=target/x86_64-unknown-uefi/debug/loader.gdt -bios build/OVMF_CODE.fd -monitor vc:2560x1440 -display gtk,zoom-to-fit=on -m 128M -device isa-debug-exit,iobase=0xf4,iosize=0x04 -serial stdio  -d int -D log.txt -no-reboot
