#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_mut_refs)]
#![feature(const_for)]
#![feature(const_trait_impl)]
#![feature(effects)]

extern crate alloc;

pub mod logger;
pub mod bits;
pub mod interrupts;
pub mod serial;
pub mod addr;
pub mod page_table;
pub mod frame_allocator;
pub mod allocator;
pub mod serial_logger;
pub mod crc;

use core::arch::asm;
use core::panic::PanicInfo;
use crate::frame_allocator::MemoryMap;
use crate::logger::FrameBufferInfo;

pub struct BootInfo {
    pub fb_info: FrameBufferInfo,
    pub rsdp_addr: u64,
    pub memory_map: MemoryMap,
    pub memory_map_next_free_frame: usize
}

pub const VIRT_MAPPING_OFFSET: u64 = 0x180_0000_0000;

#[inline]
pub unsafe fn read_u32_ptr(ptr: *mut u32, offset: u32) -> u32 {
    core::ptr::read_volatile(ptr.byte_offset(offset as isize))
}

#[inline]
pub unsafe fn write_u32_ptr(ptr: *mut u32, offset: u32, value: u32) {
    core::ptr::write_volatile(ptr.byte_offset(offset as isize), value);
}

#[inline]
pub fn get_tsc() -> u64 {
    let mut edx: u32;
    let mut eax: u32;
    unsafe { asm!("rdtsc", out("edx") edx, out("eax") eax); }
    eax as u64 | ((edx as u64) << 32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    unsafe {
        let port = 0xf4;
        let value = exit_code as u8;
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

pub fn test_panic_handler(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failed);
    loop {}
}

// our panic handler in test mode
#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}

pub trait Testable {
    fn run(&self) -> ();
}

impl<T> Testable for T
    where
        T: Fn(),
{
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }

    exit_qemu(QemuExitCode::Success);
}

#[macro_export]
macro_rules! entry_point {
    ($path:path) => {
        #[export_name = "_start"]
        pub extern "C" fn __impl_start(boot_info: &'static BootInfo) -> ! {
            // validate the signature of the program entry point
            let f: fn(&'static BootInfo) -> ! = $path;

            f(boot_info)
        }
    };
}

#[cfg(test)]
entry_point!(test_kernel_main);

/// Entry point for `cargo test`
#[cfg(test)]
fn test_kernel_main(_boot_info: &'static BootInfo) -> ! {
    //init();
    test_main();
    loop {}
}
