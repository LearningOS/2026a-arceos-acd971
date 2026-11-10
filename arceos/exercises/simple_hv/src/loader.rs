use std::io::{self, Read};
use std::fs::File;
use axhal::paging::MappingFlags;
use axhal::mem::{PAGE_SIZE_4K, phys_to_virt};
use axmm::AddrSpace;
use crate::VM_ENTRY;

pub fn load_vm_image(fname: &str, uspace: &mut AddrSpace) -> io::Result<()> {
    let mut buf = [0u8; 64];
    let img_len = load_file(fname, &mut buf)?;

    // Map the guest code page at VM_ENTRY.
    uspace.map_alloc(VM_ENTRY.into(), PAGE_SIZE_4K, MappingFlags::READ|MappingFlags::WRITE|MappingFlags::EXECUTE|MappingFlags::USER, true).unwrap();

    let (paddr, _, _) = uspace
        .page_table()
        .query(VM_ENTRY.into())
        .unwrap_or_else(|_| panic!("Mapping failed for segment: {:#x}", VM_ENTRY));

    ax_println!("paddr: {:#x}", paddr);

    unsafe {
        core::ptr::copy_nonoverlapping(
            buf.as_ptr(),
            phys_to_virt(paddr).as_mut_ptr(),
            img_len,
        );
    }

    // Map the page at guest physical address 0, and put the magic number
    // 0x6688 at offset 0x40, which will be read by the guest with
    // `ld a0, 64(zero)`.
    uspace
        .map_alloc(0usize.into(), PAGE_SIZE_4K, MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER, true)
        .unwrap();
    uspace
        .write(axhal::mem::VirtAddr::from(0x40), &0x6688usize.to_le_bytes())
        .unwrap();

    // Replace the first guest instruction `csrr a1, mhartid` (0xf14025f3),
    // which would trap as a virtual instruction in VS-mode, with
    // `ld a1, 72(zero)` (0x04803583), and pre-fill the emulated hart id
    // 0x1234 at GPA 0x48, so the guest reads the virtual hart id
    // directly from memory without trapping.
    uspace
        .write(
            axhal::mem::VirtAddr::from(VM_ENTRY),
            &0x0480_3583u32.to_le_bytes(),
        )
        .unwrap();
    uspace
        .write(axhal::mem::VirtAddr::from(0x48usize), &0x1234u64.to_le_bytes())
        .unwrap();

    Ok(())
}

fn load_file(fname: &str, buf: &mut [u8]) -> io::Result<usize> {
    ax_println!("app: {}", fname);
    let mut file = File::open(fname)?;
    let n = file.read(buf)?;
    Ok(n)
}
