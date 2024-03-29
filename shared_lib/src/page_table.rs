use core::arch::asm;
use core::ops::IndexMut;
use bitflags::bitflags;
use crate::addr::VirtAddr;

pub const PAGE_SIZE: u64 = 4096;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry {
    entry: u64,
}

impl PageTableEntry {
    #[inline]
    pub const fn new() -> Self {
        PageTableEntry { entry: 0 }
    }

    #[inline]
    pub fn set_addr(&mut self, addr: u64, flags: PageTableFlags) {
        self.entry = addr | flags.bits();
    }

    /// Returns the flags of this entry.
    #[inline]
    pub const fn flags(&self) -> PageTableFlags {
        PageTableFlags::from_bits_truncate(self.entry)
    }

    #[inline]
    pub const fn is_present(&self) -> bool {
        self.flags().contains(PageTableFlags::PRESENT)
    }

    /// Returns the physical address mapped by this entry, might be zero.
    #[inline]
    pub fn addr(&self) -> u64 {
        self.entry & 0x000f_ffff_ffff_f000
    }
}

bitflags! {
    /// Possible flags for a page table entry.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct PageTableFlags: u64 {
        /// Specifies whether the mapped frame or page table is loaded in memory.
        const PRESENT =         1;
        /// Controls whether writes to the mapped frames are allowed.
        ///
        /// If this bit is unset in a level 1 page table entry, the mapped frame is read-only.
        /// If this bit is unset in a higher level page table entry the complete range of mapped
        /// pages is read-only.
        const WRITABLE =        1 << 1;
        /// Controls whether accesses from userspace (i.e. ring 3) are permitted.
        const USER_ACCESSIBLE = 1 << 2;
        /// If this bit is set, a “write-through” policy is used for the cache, else a “write-back”
        /// policy is used.
        const WRITE_THROUGH =   1 << 3;
        /// Disables caching for the pointed entry is cacheable.
        const NO_CACHE =        1 << 4;
        /// Set by the CPU when the mapped frame or page table is accessed.
        const ACCESSED =        1 << 5;
        /// Set by the CPU on a write to the mapped frame.
        const DIRTY =           1 << 6;
        /// Specifies that the entry maps a huge frame instead of a page table. Only allowed in
        /// P2 or P3 tables.
        const HUGE_PAGE =       1 << 7;
        /// Indicates that the mapping is present in all address spaces, so it isn't flushed from
        /// the TLB on an address space switch.
        const GLOBAL =          1 << 8;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_9 =           1 << 9;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_10 =          1 << 10;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_11 =          1 << 11;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_52 =          1 << 52;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_53 =          1 << 53;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_54 =          1 << 54;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_55 =          1 << 55;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_56 =          1 << 56;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_57 =          1 << 57;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_58 =          1 << 58;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_59 =          1 << 59;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_60 =          1 << 60;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_61 =          1 << 61;
        /// Available to the OS, can be used to store additional data, e.g. custom flags.
        const BIT_62 =          1 << 62;
        /// Forbid code execution from the mapped frames.
        ///
        /// Can be only used when the no-execute page protection feature is enabled in the EFER
        /// register.
        const NO_EXECUTE =      1 << 63;
    }
}

pub const ENTRY_COUNT: u16 = 512;

#[repr(align(4096))]
#[derive(Clone, Copy)]
pub struct PageTable {
    entries: [PageTableEntry; ENTRY_COUNT as usize],
}

impl PageTable {
    pub const fn new() -> Self {
        const EMPTY: PageTableEntry = PageTableEntry::new();
        PageTable {
            entries: [EMPTY; ENTRY_COUNT as usize],
        }
    }

    pub fn clear(&mut self) {
        for entry in self.entries.iter_mut() {
            entry.set_addr(0, PageTableFlags::from_bits(0).unwrap());
        }
    }
}

impl core::ops::Index<u16> for PageTable {
    type Output = PageTableEntry;

    #[inline]
    fn index(&self, index: u16) -> &Self::Output {
        &self.entries[index as usize]
    }
}

impl core::ops::IndexMut<u16> for PageTable {
    #[inline]
    fn index_mut(&mut self, index: u16) -> &mut Self::Output {
        &mut self.entries[index as usize]
    }
}

unsafe fn create_next_table<'a>(page_table_entry: &'a mut PageTableEntry, page_tables_allocator: &'a mut impl PageTablesAllocator, offset: u64)
                                -> Result::<&'a mut PageTable, &'static str> {
    if page_table_entry.flags().contains(PageTableFlags::PRESENT) {
        let next_page_table = unsafe { &mut *((page_table_entry.addr() + offset) as *mut PageTable) };
        Ok(next_page_table)
    }
    else {
        let new_table = page_tables_allocator.allocate_page_table()?;
        page_table_entry.set_addr(new_table as *const _ as u64 - offset, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
        Ok(new_table)
    }
}

pub trait PageTablesAllocator {
    fn allocate_page_table(&mut self) -> Result::<&mut PageTable, &'static str>;
}

enum MappingMode {
    CheckFrameIsFree,
    Remapping
}

unsafe fn map_address_impl(l4_page_table: &mut PageTable, virt: VirtAddr, phys: u64, page_tables_allocator: &mut impl PageTablesAllocator, mapping_mode: MappingMode, offset: u64)
                           -> core::result::Result<(), &'static str> {
    if virt.0 % 4096 != 0 {
        return Err("Virtual address must be aligned!");
    }

    if phys % 4096 != 0 {
        return Err("Physical address must be aligned!");
    }

    log::trace!("Mapping {} -> {:#x}", virt, phys);

    let l3_page_table_entry = {
        let l3_table = create_next_table(&mut l4_page_table[virt.p4_index()], page_tables_allocator, offset)?;
        l3_table.index_mut(virt.p3_index()) as *mut PageTableEntry
    };

    log::trace!("[mapper] got l3_page_table");

    let l2_page_table_entry = {
        let l2_table = create_next_table(&mut *l3_page_table_entry, page_tables_allocator, offset)?;
        l2_table.index_mut(virt.p2_index()) as *mut PageTableEntry
    };

    log::trace!("[mapper] got l2_page_table");

    let l1_table = create_next_table(&mut *l2_page_table_entry, page_tables_allocator, offset)?;

    log::trace!("[mapper] got l1_page_table");

    let l1_entry = &mut l1_table[virt.p1_index()];

    log::trace!("[mapper] got l1_entry {:#x}", l1_entry as *const _ as u64);
    return if l1_entry.flags().contains(PageTableFlags::PRESENT) {
        if l1_entry.addr() == phys {
            log::info!("[mapper] addr {} already mapped to the same physical address. doing nothing.", virt);
            return Ok(());
        }

        match mapping_mode {
            MappingMode::CheckFrameIsFree => Err("this virtual address already mapped to another frame"),
            MappingMode::Remapping => {
                l1_entry.set_addr(phys, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
                asm!("invlpg [{}]", in(reg) phys, options(nostack, preserves_flags));
                Ok(())
            }
        }
    } else {
        l1_entry.set_addr(phys, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
        asm!("invlpg [{}]", in(reg) phys, options(nostack, preserves_flags));
        Ok(())
    }
}

pub unsafe fn map_address(l4_page_table: &mut PageTable, virt: VirtAddr, phys: u64, page_tables_allocator: &mut impl PageTablesAllocator)
                          -> core::result::Result<(), &'static str> {
    map_address_impl(l4_page_table, virt, phys, page_tables_allocator, MappingMode::CheckFrameIsFree, 0)
}

pub unsafe fn remap_address(l4_page_table: &mut PageTable, virt: VirtAddr, phys: u64, page_tables_allocator: &mut impl PageTablesAllocator)
                            -> core::result::Result<(), &'static str> {
    map_address_impl(l4_page_table, virt, phys, page_tables_allocator, MappingMode::Remapping, 0)
}

pub unsafe fn map_address_with_offset(l4_page_table: &mut PageTable, virt: VirtAddr, phys: u64, page_tables_allocator: &mut impl PageTablesAllocator, offset: u64)
                          -> core::result::Result<(), &'static str> {
    map_address_impl(l4_page_table, virt, phys, page_tables_allocator, MappingMode::CheckFrameIsFree, offset)
}

pub unsafe fn get_physical_address(l4_page_table: &PageTable, virt: VirtAddr) -> Option<u64> {
    let l4_entry = l4_page_table[virt.p4_index()];
    if !l4_entry.flags().contains(PageTableFlags::PRESENT) {
        return None;
    }

    let l3_table = & *(l4_entry.addr() as *const PageTable);
    let l3_entry = l3_table[virt.p3_index()];
    if !l3_entry.flags().contains(PageTableFlags::PRESENT) {
        return None;
    }

    let l2_table = & *(l3_entry.addr() as *const PageTable);
    let l2_entry = l2_table[virt.p2_index()];
    if !l2_entry.flags().contains(PageTableFlags::PRESENT) {
        return None;
    }

    let l1_table = & *(l2_entry.addr() as *const PageTable);
    let l1_entry = l1_table[virt.p1_index()];
    if !l1_entry.flags().contains(PageTableFlags::PRESENT) {
        return None;
    }

    Some(l1_entry.addr())
}

pub fn align_down(val: VirtAddr) -> VirtAddr {
    return val & VirtAddr::new(0xffff_ffff_ffff_f000);
}

pub fn align_down_u64(val: u64) -> u64 {
    return val & 0xffff_ffff_ffff_f000;
}