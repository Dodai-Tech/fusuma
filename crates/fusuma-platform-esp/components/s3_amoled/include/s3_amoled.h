// Minimal C shim for the Waveshare ESP32-S3-Touch-AMOLED-2.06: the 410x502 CO5300 AMOLED over QSPI and the FT3168
// touch panel over I2C. Command set, start-up sequence and pins follow Hibari's Rust drivers (hibari-drivers co5300 and
// ft3168, boards/s3-amoled-206), which follow Waveshare's Arduino_CO5300.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"

#define S3_LCD_W 410
#define S3_LCD_H 502

// Reset and start the panel, then the touch controller. Call once. A touch controller that does not answer is not an
// error: the panel still works and `s3_touch_read` reports failures.
esp_err_t s3_amoled_init(void);

// Sends the pixels of the rectangle [x0, x1) x [y0, y1), row by row, as big-endian RGB565. The panel takes rectangles
// that start and end on even pixels. Returns before the transfer ends: `pixels` must stay untouched until the next
// call of any `s3_amoled_*` function, which waits for it. `pixels` must be DMA-capable internal memory.
esp_err_t s3_amoled_draw(uint16_t x0, uint16_t y0, uint16_t x1, uint16_t y1, const uint16_t *pixels);

// Waits until every pixel transfer has ended.
esp_err_t s3_amoled_wait(void);

// Brightness, 0 (dark) to 255.
esp_err_t s3_amoled_brightness(uint8_t level);

// One FT3168 sample. Returns false when the bus failed (the controller answers NAK while idle, so a few failures a
// minute are normal); otherwise sets `*down` and, when down, the position in panel pixels.
bool s3_touch_read(bool *down, uint16_t *x, uint16_t *y);
