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

    Ok(())
}

fn load_file(fname: &str, buf: &mut [u8]) -> io::Result<usize> {
    ax_println!("app: {}", fname);
    let mut file = File::open(fname)?;
    let n = file.read(buf)?;
    Ok(n)
}
