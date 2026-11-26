//! The rails and latches a board's `[power]` table names, switched on before anything talks to a part: GPIOs driven high then
//! low, then register writes to a chip on I2C (an I/O expander or a PMIC, with its reset lines and power switches). The bus is
//! opened and freed per write, so the touch and display drivers can open their own on the same pins afterwards.

use std::time::Duration;

use esp_idf_svc::sys::fusuma_esp as ffi;

use crate::board::{POWER_HIGH, POWER_I2C, POWER_I2C_WRITES, POWER_LOW};

/// Switches the rails on, in the board file's order; a board with no `[power]` does nothing.
///
/// # Errors
/// When a register write to the rail chip fails.
pub(crate) fn rails() -> anyhow::Result<()> {
    for &g in POWER_HIGH {
        // SAFETY: a plain GPIO setup at startup.
        unsafe { ffi::fusuma_gpio_output(g, 1) };
    }
    for &g in POWER_LOW {
        // SAFETY: as above.
        unsafe { ffi::fusuma_gpio_output(g, 0) };
    }
    let (sda, scl) = POWER_I2C;
    for &[addr, reg, value, mask, delay_ms] in POWER_I2C_WRITES {
        // SAFETY: a short blocking I2C write, on a bus the call opens and closes itself.
        let err = unsafe {
            ffi::fusuma_i2c_write_reg(
                sda,
                scl,
                i32::from(addr),
                i32::from(reg),
                i32::from(value),
                i32::from(mask),
            )
        };
        anyhow::ensure!(
            err == 0,
            "rail chip {addr:#x} register {reg:#x}: esp error {err}"
        );
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(u64::from(delay_ms)));
        }
    }
    if !POWER_HIGH.is_empty() || !POWER_LOW.is_empty() || !POWER_I2C_WRITES.is_empty() {
        // Let the rails settle before the first read or write to a part.
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
