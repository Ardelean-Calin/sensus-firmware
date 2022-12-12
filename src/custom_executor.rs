use core::arch::asm;
use core::marker::PhantomData;
use core::ptr;

use embassy_executor::{raw, Spawner};
use nrf52832_pac as pac;

/// Thread mode executor, using WFE/SEV.
///
/// This is the simplest and most common kind of executor. It runs on
/// thread mode (at the lowest priority level), and uses the `WFE` ARM instruction
/// to sleep when it has no more work to do. When a task is woken, a `SEV` instruction
/// is executed, to make the `WFE` exit from sleep and poll the task.
///
/// This executor allows for ultra low power consumption for chips where `WFE`
/// triggers low-power sleep without extra steps. If your chip requires extra steps,
/// you may use [`raw::Executor`] directly to program custom behavior.
pub struct Executor {
    inner: raw::Executor,
    not_send: PhantomData<*mut ()>,
}

impl Executor {
    /// Create a new Executor.
    pub fn new() -> Self {
        Self {
            inner: raw::Executor::new(|_| unsafe { asm!("sev") }, ptr::null_mut()),
            not_send: PhantomData,
        }
    }

    /// Run the executor.
    ///
    /// The `init` closure is called with a [`Spawner`] that spawns tasks on
    /// this executor. Use it to spawn the initial task(s). After `init` returns,
    /// the executor starts running the tasks.
    ///
    /// To spawn more tasks later, you may keep copies of the [`Spawner`] (it is `Copy`),
    /// for example by passing it as an argument to the initial tasks.
    ///
    /// This function requires `&'static mut self`. This means you have to store the
    /// Executor instance in a place where it'll live forever and grants you mutable
    /// access. There's a few ways to do this:
    ///
    /// - a [StaticCell](https://docs.rs/static_cell/latest/static_cell/) (safe)
    /// - a `static mut` (unsafe)
    /// - a local variable in a function you know never returns (like `fn main() -> !`), upgrading its lifetime with `transmute`. (unsafe)
    ///
    /// This function never returns.
    pub fn run(&'static mut self, init: impl FnOnce(Spawner)) -> ! {
        init(self.inner.spawner());

        loop {
            unsafe {
                self.inner.poll();

                #[allow(unused_doc_comments)]
                /**
                 * We need to apply this work-around to get low power consumption, as there is a bug in nRF52832 causing
                 * really high current draw when using S132 and WFE. See https://shorturl.at/FHTUV and https://shorturl.at/fxAIM
                 *
                 * NRF_MWU->REGIONENCLR = MWU_AccessWatchMask; // Disable write access watch in region[0] and PREGION[0]
                 * __WFE();
                 * __NOP(); __NOP(); __NOP(); __NOP();
                 * // Errata 75: MWU Enable
                 * NRF_MWU->REGIONENSET = MWU_AccessWatchMask;
                 */
                /* MWU Disable */
                let mwu = &*pac::MWU::ptr();

                // Disable region monitoring
                mwu.regionenclr.write(|w| w.bits(0xFFFFFFFF));
                asm!("wfe");
                asm!("nop");
                asm!("nop");
                asm!("nop");
                asm!("nop");

                mwu.regionenset.write(|w| w.bits(0x10000001));
            };
        }
    }
}
