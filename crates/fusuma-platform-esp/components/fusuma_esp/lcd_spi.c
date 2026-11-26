// ST7789 and GC9A01 LCDs on plain SPI with a data/command line. The start-up of the ST7789 is Waveshare's for the
// ESP32-C6-LCD-1.47 (waveshareteam/ESP32-C6-LCD-1.47, its Vernon_ST7789T driver); the GC9A01's is the long vendor sequence from
// Arduino_GFX's Arduino_GC9A01, which Waveshare's 1.28 inch boards use. Read, not run on a board.
#include "fusuma_esp.h"

#include "driver/gpio.h"
#include "driver/spi_master.h"
#include "esp_lcd_panel_io.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define LCD_HOST SPI2_HOST

#define SLPOUT 0x11
#define INVOFF 0x20
#define INVON 0x21
#define DISPON 0x29
#define CASET 0x2A
#define RASET 0x2B
#define RAMWR 0x2C
#define MADCTL 0x36
#define COLMOD 0x3A

static esp_lcd_panel_io_handle_t s_io;
static uint16_t s_x_offset, s_y_offset;

static void sleep_ms(uint32_t ms) {
    vTaskDelay(pdMS_TO_TICKS(ms) ? pdMS_TO_TICKS(ms) : 1);
}

typedef struct {
    uint8_t reg;
    uint8_t len;
    const uint8_t *data;
    uint16_t delay_ms;
} step_t;

// A register with data, and one without.
#define D(reg, delay, ...) {reg, sizeof((const uint8_t[]){__VA_ARGS__}), (const uint8_t[]){__VA_ARGS__}, delay}
#define C(reg, delay) {reg, 0, NULL, delay}

static esp_err_t run(const step_t *steps, size_t n) {
    for (size_t i = 0; i < n; i++) {
        esp_err_t err = esp_lcd_panel_io_tx_param(s_io, steps[i].reg, steps[i].data, steps[i].len);
        if (err != ESP_OK) return err;
        if (steps[i].delay_ms) sleep_ms(steps[i].delay_ms);
    }
    return ESP_OK;
}

// The ST7789 after sleep-out, memory access and pixel format: porch, gate and VCOM voltages, power, gamma. (The vendor sends
// the first byte of the power control 1 only; the chip takes two, 0xA4 0xA1, as its data sheet and the usual drivers have it.)
static const step_t ST7789_UP[] = {
    D(0xB0, 0, 0x00, 0xE8),
    D(0xB2, 0, 0x0C, 0x0C, 0x00, 0x33, 0x33),
    D(0xB7, 0, 0x75),
    D(0xBB, 0, 0x1A),
    D(0xC0, 0, 0x80),
    D(0xC2, 0, 0x01, 0xFF),
    D(0xC3, 0, 0x13),
    D(0xC4, 0, 0x20),
    D(0xC6, 0, 0x0F),
    D(0xD0, 0, 0xA4, 0xA1),
    D(0xE0, 0, 0xD0, 0x0D, 0x14, 0x0D, 0x0D, 0x09, 0x38, 0x44, 0x4E, 0x3A, 0x17, 0x18, 0x2F, 0x30),
    D(0xE1, 0, 0xD0, 0x09, 0x0F, 0x08, 0x07, 0x14, 0x37, 0x44, 0x4D, 0x38, 0x15, 0x16, 0x2C, 0x2E),
};

// The GC9A01 before sleep-out: inner registers, then gamma.
static const step_t GC9A01_UP[] = {
    C(0xEF, 0),
    D(0xEB, 0, 0x14),
    C(0xFE, 0),
    C(0xEF, 0),
    D(0xEB, 0, 0x14),
    D(0x84, 0, 0x40), D(0x85, 0, 0xFF), D(0x86, 0, 0xFF), D(0x87, 0, 0xFF), D(0x88, 0, 0x0A), D(0x89, 0, 0x21),
    D(0x8A, 0, 0x00), D(0x8B, 0, 0x80), D(0x8C, 0, 0x01), D(0x8D, 0, 0x01), D(0x8E, 0, 0xFF), D(0x8F, 0, 0xFF),
    D(0xB6, 0, 0x00, 0x20),
    D(COLMOD, 0, 0x05),
    D(0x90, 0, 0x08, 0x08, 0x08, 0x08),
    D(0xBD, 0, 0x06),
    D(0xBC, 0, 0x00),
    D(0xFF, 0, 0x60, 0x01, 0x04),
    D(0xC3, 0, 0x13), D(0xC4, 0, 0x13), D(0xC9, 0, 0x22), D(0xBE, 0, 0x11),
    D(0xE1, 0, 0x10, 0x0E),
    D(0xDF, 0, 0x21, 0x0C, 0x02),
    D(0xF0, 0, 0x45, 0x09, 0x08, 0x08, 0x26, 0x2A),
    D(0xF1, 0, 0x43, 0x70, 0x72, 0x36, 0x37, 0x6F),
    D(0xF2, 0, 0x45, 0x09, 0x08, 0x08, 0x26, 0x2A),
    D(0xF3, 0, 0x43, 0x70, 0x72, 0x36, 0x37, 0x6F),
    D(0xED, 0, 0x1B, 0x0B),
    D(0xAE, 0, 0x77),
    D(0xCD, 0, 0x63),
    D(0x70, 0, 0x07, 0x07, 0x04, 0x0E, 0x0F, 0x09, 0x07, 0x08, 0x03),
    D(0xE8, 0, 0x34),
    D(0x62, 0, 0x18, 0x0D, 0x71, 0xED, 0x70, 0x70, 0x18, 0x0F, 0x71, 0xEF, 0x70, 0x70),
    D(0x63, 0, 0x18, 0x11, 0x71, 0xF1, 0x70, 0x70, 0x18, 0x13, 0x71, 0xF3, 0x70, 0x70),
    D(0x64, 0, 0x28, 0x29, 0xF1, 0x01, 0xF1, 0x00, 0x07),
    D(0x66, 0, 0x3C, 0x00, 0xCD, 0x67, 0x45, 0x45, 0x10, 0x00, 0x00, 0x00),
    D(0x67, 0, 0x00, 0x3C, 0x00, 0x00, 0x00, 0x01, 0x54, 0x10, 0x32, 0x98),
    D(0x74, 0, 0x10, 0x85, 0x80, 0x00, 0x00, 0x4E, 0x00),
    D(0x98, 0, 0x3E, 0x07),
};

esp_err_t fusuma_lcd_init(const fusuma_lcd_config_t *cfg) {
    s_x_offset = cfg->x_offset;
    s_y_offset = cfg->y_offset;
    esp_err_t err = ESP_OK;
    if (cfg->rst >= 0) {
        gpio_config_t rst = {.pin_bit_mask = 1ULL << cfg->rst, .mode = GPIO_MODE_OUTPUT};
        err = gpio_config(&rst);
        if (err != ESP_OK) return err;
    }
    spi_bus_config_t bus = {
        .sclk_io_num = cfg->sck,
        .mosi_io_num = cfg->mosi,
        .miso_io_num = -1,
        .quadwp_io_num = -1,
        .quadhd_io_num = -1,
        .max_transfer_sz = (int)cfg->max_transfer,
    };
    err = spi_bus_initialize(LCD_HOST, &bus, SPI_DMA_CH_AUTO);
    if (err != ESP_OK) return err;
    esp_lcd_panel_io_spi_config_t io = {
        .cs_gpio_num = cfg->cs,
        .dc_gpio_num = cfg->dc,
        .spi_mode = 0,
        .pclk_hz = cfg->pclk_hz,
        .trans_queue_depth = 4,
        .lcd_cmd_bits = 8,
        .lcd_param_bits = 8,
    };
    err = esp_lcd_new_panel_io_spi((esp_lcd_spi_bus_handle_t)LCD_HOST, &io, &s_io);
    if (err != ESP_OK) return err;

    if (cfg->rst >= 0) {
        gpio_set_level(cfg->rst, 1);
        sleep_ms(10);
        gpio_set_level(cfg->rst, 0);
        sleep_ms(20);
        gpio_set_level(cfg->rst, 1);
        sleep_ms(120);
    }
    const uint8_t madctl = cfg->madctl;
    const uint8_t colmod = 0x55;
    if (cfg->kind == FUSUMA_LCD_GC9A01) {
        // Registers first, then wake, then the scan order and the picture's polarity.
        err = run(GC9A01_UP, sizeof GC9A01_UP / sizeof GC9A01_UP[0]);
        if (err != ESP_OK) return err;
        err = esp_lcd_panel_io_tx_param(s_io, SLPOUT, NULL, 0);
        if (err != ESP_OK) return err;
        sleep_ms(120);
        err = esp_lcd_panel_io_tx_param(s_io, MADCTL, &madctl, 1);
    } else {
        err = esp_lcd_panel_io_tx_param(s_io, SLPOUT, NULL, 0);
        if (err != ESP_OK) return err;
        sleep_ms(100);
        err = esp_lcd_panel_io_tx_param(s_io, MADCTL, &madctl, 1);
        if (err != ESP_OK) return err;
        err = esp_lcd_panel_io_tx_param(s_io, COLMOD, &colmod, 1);
        if (err != ESP_OK) return err;
        if (cfg->tuned) err = run(ST7789_UP, sizeof ST7789_UP / sizeof ST7789_UP[0]);
    }
    if (err != ESP_OK) return err;
    err = esp_lcd_panel_io_tx_param(s_io, cfg->invert ? INVON : INVOFF, NULL, 0);
    if (err != ESP_OK) return err;
    err = esp_lcd_panel_io_tx_param(s_io, DISPON, NULL, 0);
    sleep_ms(20);
    return err;
}

esp_err_t fusuma_lcd_draw(uint16_t x0, uint16_t y0, uint16_t x1, uint16_t y1, const uint16_t *pixels) {
    if (!s_io || x1 <= x0 || y1 <= y0) return ESP_ERR_INVALID_ARG;
    const uint16_t px0 = x0 + s_x_offset, px1 = x1 - 1 + s_x_offset;
    const uint16_t py0 = y0 + s_y_offset, py1 = y1 - 1 + s_y_offset;
    const uint8_t cols[4] = {px0 >> 8, px0 & 0xff, px1 >> 8, px1 & 0xff};
    const uint8_t rows[4] = {py0 >> 8, py0 & 0xff, py1 >> 8, py1 & 0xff};
    // A register write waits for the pixel transfer before it, so the window never changes under one.
    esp_err_t err = esp_lcd_panel_io_tx_param(s_io, CASET, cols, 4);
    if (err != ESP_OK) return err;
    err = esp_lcd_panel_io_tx_param(s_io, RASET, rows, 4);
    if (err != ESP_OK) return err;
    return esp_lcd_panel_io_tx_color(s_io, RAMWR, pixels, (size_t)(x1 - x0) * (size_t)(y1 - y0) * 2);
}

esp_err_t fusuma_lcd_wait(void) {
    // A register write waits for every pixel transfer; this one is the panel's NOP.
    return s_io ? esp_lcd_panel_io_tx_param(s_io, 0x00, NULL, 0) : ESP_ERR_INVALID_STATE;
}
