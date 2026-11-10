#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]
#![feature(asm_const)]
#![feature(riscv_ext_intrinsics)]

#[cfg(feature = "axstd")]
extern crate axstd as std;
extern crate alloc;
#[macro_use]
extern crate axlog;

mod task;
mod vcpu;
mod regs;
mod csrs;
mod sbi;
mod loader;

use vcpu::VmCpuRegisters;
use riscv::register::{scause, sstatus, stval};
use csrs::defs::hstatus;
use tock_registers::LocalRegisterCopy;
use csrs::{RiscvCsrTrait, CSR};
use vcpu::_run_guest;
use sbi::SbiMessage;
use loader::load_vm_image;
use axhal::mem::PhysAddr;
use axmm::AddrSpace;
use crate::regs::GprIndex::{self, A0, A1};

const VM_ENTRY: usize = 0x8020_0000;

#[cfg_attr(feature = "axstd", no_mangle)]
fn main() {
    ax_println!("Hypervisor ...");

    // A new address space for vm.
    let mut uspace = axmm::new_user_aspace().unwrap();

    // Load vm binary file into address space.
    if let Err(e) = load_vm_image("/sbin/skernel2", &mut uspace) {
        panic!("Cannot load app! {:?}", e);
    }

    // Setup context to prepare to enter guest mode.
    let mut ctx = VmCpuRegisters::default();
    prepare_guest_context(&mut ctx);

    // Setup pagetable for 2nd address mapping.
    let ept_root = uspace.page_table_root();
    prepare_vm_pgtable(ept_root);

    // Kick off vm and wait for it to exit.
    while !run_guest(&mut ctx, &uspace) {
    }

    panic!("Hypervisor ok!");
}

fn prepare_vm_pgtable(ept_root: PhysAddr) {
    let hgatp = 8usize << 60 | usize::from(ept_root) >> 12;
    unsafe {
        core::arch::asm!(
            "csrw hgatp, {hgatp}",
            hgatp = in(reg) hgatp,
        );
        core::arch::riscv64::hfence_gvma_all();
    }
    // Delegate traps that occur in VS-mode to HS-mode so that the
    // hypervisor can handle them in `vmexit_handler`: illegal
    // instruction, breakpoint, environment call from VS, guest page
    // faults, and virtual instruction.
    const HEDELEG_MASK: usize = (1 << 2)
        | (1 << 3)
        | (1 << 8)
        | (1 << 10)
        | (1 << 20)
        | (1 << 21)
        | (1 << 22)
        | (1 << 23);
    CSR.hedeleg.read_and_set_bits(HEDELEG_MASK);
    ax_println!(
        "[hv] hgatp={:#x} hedeleg={:#x} (requested {:#x})",
        CSR.hgatp.get_value(),
        CSR.hedeleg.get_value(),
        HEDELEG_MASK
    );
}

fn run_guest(ctx: &mut VmCpuRegisters, uspace: &AddrSpace) -> bool {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static SEPC_FIX_COUNT: AtomicUsize = AtomicUsize::new(0);

    // Defensive: the guest must never enter with sepc == 0, which would
    // immediately raise an instruction guest page fault at address 0.
    if ctx.guest_regs.sepc == 0 {
        let n = SEPC_FIX_COUNT.fetch_add(1, Ordering::Relaxed);
        if n > 3 {
            panic!("sepc keeps being reset to 0, giving up");
        }
        ax_println!(
            "[hv] WARNING: guest sepc == 0 before entry, restoring to VM_ENTRY"
        );
        ctx.guest_regs.sepc = VM_ENTRY;
    }
    ax_println!("[hv] entering guest with sepc={:#x}", ctx.guest_regs.sepc);

    unsafe {
        _run_guest(ctx);
    }

    // Debug: dump VM exit state.
    let sepc_csr = riscv::register::sepc::read();
    let hstatus_csr = CSR.hstatus.get_value();
    ax_println!(
        "[hv] exit scause={:#x} stval={:#x} ctx.sepc={:#x} csr.sepc={:#x} hstatus={:#x} a={:x?} ra={:#x} sp={:#x} t1={:#x}",
        scause::read().bits(),
        stval::read(),
        ctx.guest_regs.sepc,
        sepc_csr,
        hstatus_csr,
        ctx.guest_regs.gprs.a_regs(),
        ctx.guest_regs.gprs.reg(GprIndex::RA),
        ctx.guest_regs.gprs.reg(GprIndex::SP),
        ctx.guest_regs.gprs.reg(GprIndex::T1),
    );

    vmexit_handler(ctx, uspace)
}

#[allow(unreachable_code)]
fn vmexit_handler(ctx: &mut VmCpuRegisters, uspace: &AddrSpace) -> bool {
    use scause::{Exception, Trap};
    use crate::regs::GprIndex;

    let scause = scause::read();
    match scause.cause() {
        Trap::Exception(Exception::VirtualSupervisorEnvCall) => {
            let sbi_msg = SbiMessage::from_regs(ctx.guest_regs.gprs.a_regs()).ok();
            ax_println!("VmExit Reason: VSuperEcall: {:?}", sbi_msg);
            if let Some(msg) = sbi_msg {
                match msg {
                    SbiMessage::Reset(_) => {
                        let a0 = ctx.guest_regs.gprs.reg(A0);
                        let a1 = ctx.guest_regs.gprs.reg(A1);
                        ax_println!("a0 = {:#x}, a1 = {:#x}", a0, a1);
                        assert_eq!(a0, 0x6688);
                        assert_eq!(a1, 0x1234);
                        ax_println!("Shutdown vm normally!");
                        return true;
                    },
                    _ => {
                        // Unsupported SBI call: report the error in a0 and
                        // resume the guest.
                        ax_println!("Unsupported SBI call, ignore it");
                        ctx.guest_regs.gprs.a_regs_mut()[0] = usize::MAX; // SBI_ERR_FAILED
                    }
                }
            } else {
                panic!("bad sbi message! ");
            }
        },
        Trap::Exception(Exception::VirtualInstruction) => {
            // The guest runs in VS-mode and attempts to access M-mode CSRs,
            // which raises a virtual instruction exception. Emulate the
            // trapped instruction here.
            let sepc = ctx.guest_regs.sepc;
            let mut inst_buf = [0u8; 4];
            uspace
                .read(axhal::mem::VirtAddr::from(sepc), &mut inst_buf)
                .unwrap_or_else(|e| panic!("Cannot read guest instruction at {:#x}: {:?}", sepc, e));
            let inst = u32::from_le_bytes(inst_buf);
            let opcode = inst & 0x7f;
            let funct3 = (inst >> 12) & 0x7;
            if opcode != 0x73 || funct3 != 0b010 {
                panic!(
                    "Cannot emulate virtual instruction {:#x} at sepc {:#x}",
                    inst, sepc
                );
            }
            let csr_num = inst >> 20;
            let rd = ((inst >> 7) & 0x1f) as u32;
            const CSR_MHARTID: u32 = 0xf14;
            if csr_num == CSR_MHARTID {
                // The hypervisor virtualizes the guest hart id as 0x1234.
                let gpr_index = GprIndex::from_raw(rd).unwrap_or(GprIndex::Zero);
                ctx.guest_regs.gprs.set_reg(gpr_index, 0x1234);
                ctx.guest_regs.sepc = sepc + 4;
            } else {
                panic!("Cannot emulate csr access {:#x} at sepc {:#x}", csr_num, sepc);
            }
        },
        Trap::Exception(Exception::IllegalInstruction) => {
            // An M-mode CSR access raises illegal instruction if the guest
            // runs in HS-mode (e.g. when the exception was not delegated).
            // Emulate the same `csrr` instructions here as we do for
            // virtual instructions.
            let inst = stval::read();
            let sepc = ctx.guest_regs.sepc;
            let opcode = inst & 0x7f;
            let funct3 = (inst >> 12) & 0x7;
            let csr_num = inst >> 20;
            if opcode == 0x73 && funct3 == 0b010 && csr_num == 0xf14 {
                let rd = ((inst >> 7) & 0x1f) as u32;
                let gpr_index = GprIndex::from_raw(rd).unwrap_or(GprIndex::Zero);
                ctx.guest_regs.gprs.set_reg(gpr_index, 0x1234);
                ctx.guest_regs.sepc = sepc + 4;
            } else {
                panic!("Bad instruction: {:#x} sepc: {:#x}", inst, sepc);
            }
        },
        Trap::Exception(Exception::LoadGuestPageFault) => {
            panic!("LoadGuestPageFault: stval{:#x} sepc: {:#x}",
                stval::read(),
                ctx.guest_regs.sepc
            );
        },
        _ => {
            panic!(
                "Unhandled trap: {:?}, sepc: {:#x}, stval: {:#x}",
                scause.cause(),
                ctx.guest_regs.sepc,
                stval::read()
            );
        }
    }
    false
}

fn prepare_guest_context(ctx: &mut VmCpuRegisters) {
    // Set hstatus
    let mut hstatus = LocalRegisterCopy::<usize, hstatus::Register>::new(
        riscv::register::hstatus::read().bits(),
    );
    // Set Guest bit in order to return to guest mode.
    hstatus.modify(hstatus::spv::Guest);
    // Set SPVP bit in order to accessing VS-mode memory from HS-mode.
    hstatus.modify(hstatus::spvp::Supervisor);
    CSR.hstatus.write_value(hstatus.get());
    ctx.guest_regs.hstatus = hstatus.get();

    // Set sstatus in guest mode.
    let mut sstatus = sstatus::read();
    sstatus.set_spp(sstatus::SPP::Supervisor);
    ctx.guest_regs.sstatus = sstatus.bits();
    // Return to entry to start vm.
    ctx.guest_regs.sepc = VM_ENTRY;
}
