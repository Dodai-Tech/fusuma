// Fusuma's ESP-IDF drivers, one per controller (not per board): a board file names the controllers and their pins,
// and the Rust side passes them in. Every function returns ESP_OK or the ESP-IDF error; a pin of -1 is not wired.
// All I2C goes through the new i2c_master driver: ESP-IDF aborts at boot when the legacy and the new driver are
// both in one image.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"

// ── CO5300: AMOLED over QSPI, no frame buffer the chip writes; takes rectangles. ─────────────────────────────────
typedef struct {
    int sck, d0, d1, d2, d3, cs, rst;
    uint32_t pclk_hz;
    // Where the visible area starts in the panel's memory.
    uint16_t x_offset, y_offset;
    // The largest pixel transfer in bytes (the band buffer).
    size_t max_transfer;
} fusuma_co5300_config_t;

esp_err_t fusuma_co5300_init(const fusuma_co5300_config_t *cfg);
// Sends [x0, x1) x [y0, y1), row by row, big-endian RGB565. Returns before the transfer ends: `pixels` must stay
// untouched until the next fusuma_co5300_* call, which waits for it. `pixels` must be DMA-capable internal memory.
esp_err_t fusuma_co5300_draw(uint16_t x0, uint16_t y0, uint16_t x1, uint16_t y1, const uint16_t *pixels);
// Waits until every pixel transfer has ended.
esp_err_t fusuma_co5300_wait(void);
// Brightness, 0 to 255.
esp_err_t fusuma_co5300_brightness(uint8_t level);

// ── EK79007: MIPI DSI panel with a frame buffer the chip writes (ESP32-P4 only; 1024x600). ──────────────────────
typedef struct {
    int rst;
    uint8_t lanes;
    uint16_t lane_mbps;
    // The LDO channel and voltage that power the DSI PHY; a channel of -1 for none.
    int phy_ldo_channel;
    uint16_t phy_ldo_mv;
} fusuma_ek79007_config_t;

esp_err_t fusuma_ek79007_init(const fusuma_ek79007_config_t *cfg);
// The frame buffer, width x height native RGB565, scanned out by DMA; NULL before init.
uint16_t *fusuma_ek79007_framebuffer(void);
// Writes rows [y0, y1) of the frame buffer back from the cache, so the DMA sees them.
esp_err_t fusuma_ek79007_flush_rows(uint32_t y0, uint32_t y1);

// ── Touch controllers on I2C. ────────────────────────────────────────────────────────────────────────────────────
typedef struct {
    int sda, scl, rst, irq;
    uint32_t hz;
    // The screen size the controller reports in.
    uint16_t width, height;
} fusuma_touch_config_t;

esp_err_t fusuma_ft3168_init(const fusuma_touch_config_t *cfg);
esp_err_t fusuma_gt911_init(const fusuma_touch_config_t *cfg);
// One sample. False when the bus failed; otherwise `*down`, and when down the position as the controller reports it.
bool fusuma_ft3168_read(bool *down, uint16_t *x, uint16_t *y);
bool fusuma_gt911_read(bool *down, uint16_t *x, uint16_t *y);

// ── Backlight on an LEDC PWM channel. ────────────────────────────────────────────────────────────────────────────
esp_err_t fusuma_ledc_backlight_init(int pin, bool invert, uint32_t freq_hz);
// 0 to 100.
void fusuma_ledc_backlight_set(int percent);

// ── E-paper panel on SPI (SSD1677 and relatives): a write-only bus plus a data/command line, a reset and a busy line. ──
typedef struct {
    int sck, mosi, miso, cs, dc, rst, busy;
    // A pin that switches the panel's power (driven high); -1 for none.
    int en;
    uint32_t hz;
} fusuma_epd_config_t;

esp_err_t fusuma_epd_init(const fusuma_epd_config_t *cfg);
// One command byte (the data/command line low).
esp_err_t fusuma_epd_command(uint8_t command);
// Data bytes (the data/command line high), in pieces the bus takes; `data` may be in PSRAM.
esp_err_t fusuma_epd_data(const uint8_t *data, size_t len);
// Pulses reset: low for 10 ms, high, 10 ms more.
esp_err_t fusuma_epd_reset(void);
// Waits until the panel is done: the busy line is low when it is idle (`active_low` 0, the SSD16xx family) or high when it is
// idle (`active_low` 1, the UC81xx family, which is low while it works); ESP_ERR_TIMEOUT after `timeout_ms`.
esp_err_t fusuma_epd_wait_busy(uint32_t timeout_ms, int active_low);

// ── Plain GPIO, for power pins and keys. ─────────────────────────────────────────────────────────────────────────
// Makes `pin` an output at `level` (0 or 1).
esp_err_t fusuma_gpio_output(int pin, int level);
// Writes `value` to `reg` of the I2C chip at `addr` (7-bit) on the bus `sda`/`scl`, then frees the bus so a driver can open its own
// on the same pins (an I/O expander or PMIC switching a rail before the rest starts).
esp_err_t fusuma_i2c_write_reg(int sda, int scl, int addr, int reg, int value);
// Makes `pin` an input with the pull-up on.
esp_err_t fusuma_gpio_input_pullup(int pin);
// The level of `pin`, 0 or 1.
int fusuma_gpio_read(int pin);

// Reads one byte from the e-paper controller by bit-banging the clock and the (shared) data line, with the SPI peripheral
// released for the read and restored after (the controller has no separate read line). `sck` and `mosi` are the pins of
// the config. Used for the chip id and the temperature.
esp_err_t fusuma_epd_read_byte(uint8_t *value);
