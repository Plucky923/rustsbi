//! IMSIC IPI device and per-hart interrupt-file initialization.
//!
//! # References
//!
//! - Specification: [RISC-V AIA 1.0](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html),
//!   sections 2.1.5 and 2.1.8 — IMSIC MMIO pages and interrupt-file setup.

use alloc::vec::Vec;
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{Iid, IpiDelivery, IpiDevice, IpiRequest},
    memory::{MemoryRegistry, MmioRegion},
};

use crate::platform::ImsicInfo;
use crate::platform::qemu_aplic::QemuAplicConfig;

/// FDT `compatible` strings identifying an IMSIC interrupt controller.
pub(crate) const IMSIC_COMPATIBLES: [&str; 1] = ["riscv,imsics"];

/// Page shift of one IMSIC interrupt file.
const IMSIC_FILE_PAGE_SHIFT: u32 = 12;
pub(crate) const IMSIC_FILE_SPAN: usize = 1usize << IMSIC_FILE_PAGE_SHIFT;

/// MMIO offset of the `seteipnum_le` register.
const SETEIPNUM_LE: usize = 0x0000;

/// IMSIC-backed IPI device delivering software interrupts as MSIs to each
/// hart's machine-level interrupt file.
pub(super) struct ImsicDevice {
    ipi_iid: Iid,
    num_iids: u16,
    hart_files: Vec<MmioRegion>,
}

impl IpiDevice for ImsicDevice {
    #[inline(always)]
    fn send(&self, req: IpiRequest) -> Result<()> {
        for hart_id in req.harts() {
            let hart = HartId::from_raw(hart_id).map_err(|_| Error::IoError)?;
            let file = self.hart_files.get(hart.index()).ok_or(Error::IoError)?;
            file.write(SETEIPNUM_LE, self.ipi_iid.number() as u32)
                .map_err(|_| Error::IoError)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn delivery(&self) -> IpiDelivery {
        IpiDelivery::Imsic {
            iid: self.ipi_iid,
            num_iids: self.num_iids,
        }
    }
}

/// Binds the selected AIA interrupt devices to their MMIO windows.
pub(super) fn bind(
    imsic: &ImsicInfo,
    aplic_config: Option<QemuAplicConfig>,
    memory: &mut MemoryRegistry,
) -> Result<ImsicDevice> {
    // No fallback is permitted after the first MMIO window is issued. All
    // hardware capability checks above therefore precede initialization.
    let hart_files: Result<Vec<_>> = imsic
        .hart_files
        .iter()
        .map(|register_range| memory.acquire(*register_range))
        .collect();
    let device = ImsicDevice {
        ipi_iid: imsic.ipi_iid,
        num_iids: imsic.num_iids,
        hart_files: hart_files?,
    };

    if let Some(aplic_config) = aplic_config {
        aplic_config.bind(memory)?;
    }

    Ok(device)
}
