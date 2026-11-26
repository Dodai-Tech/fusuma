// AMOLED panels on QSPI: the CO5300, the SH8601 and the RM690B0 share the transport (a register write on one line, pixels
// on four) and differ in their start-up and a few numbers. Start-up sequences follow Hibari's Rust driver (hibari-drivers
// co5300) and Waveshare's own firmware for each board (waveshareteam/ESP32-S3-Touch-AMOLED-*), read, not run, for the SH8601
// and the RM690B0.
#include "fusuma_esp.h"

#include "driver/gpio.h"
#include "driver/spi_master.h"
#include "esp_lcd_panel_io.h"
#include "esp_rom_sys.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define LCD_HOST SPI2_HOST

// QSPI opcodes: a register write on one line, a pixel write with the data on four.
#define OP_WRITE 0x02
#define OP_WRITE_QUAD 0x32

#define SWRESET 0x01
#define SLPOUT 0x11
#define INVOFF 0x20
#define DISPON 0x29
#define CASET 0x2A
#define PASET 0x2B
#define RAMWR 0x2C
#define RAMWR_CONTINUE 0x3C
#define BRIGHTNESS 0x51

// What the SH8601's ID register (0xDA) reads on that chip; the CO5300 reads something else (0xFF on the boards seen).
#define SH8601_ID 0x86

static esp_lcd_panel_io_handle_t s_io;
static uint16_t s_x_offset, s_y_offset;

static int cmd(uint8_t opcode, uint8_t reg) {
    return (int)(((uint32_t)opcode << 24) | ((uint32_t)reg << 8));
}

static esp_err_t write_reg(uint8_t reg, const uint8_t *data, size_t len) {
    return esp_lcd_panel_io_tx_param(s_io, cmd(OP_WRITE, reg), data, len);
}

static void sleep_ms(uint32_t ms) {
    vTaskDelay(pdMS_TO_TICKS(ms) ? pdMS_TO_TICKS(ms) : 1);
}

// One start-up step: a register, up to four bytes, and a pause after it. `len` 0 sends the register alone.
typedef struct {
    uint8_t reg;
    uint8_t len;
    uint8_t data[4];
    uint16_t delay_ms;
} step_t;

static esp_err_t run(const step_t *steps, size_t n) {
    for (size_t i = 0; i < n; i++) {
        esp_err_t err = write_reg(steps[i].reg, steps[i].len ? steps[i].data : NULL, steps[i].len);
        if (err != ESP_OK) return err;
        if (steps[i].delay_ms) sleep_ms(steps[i].delay_ms);
    }
    return ESP_OK;
}

// The CO5300 (Hibari's sequence). The display goes on after the first five registers.
static const step_t CO5300_UP[] = {
    {0xFE, 1, {0x00}, 0}, {0xC4, 1, {0x80}, 0}, {0x3A, 1, {0x55}, 0}, {0x53, 1, {0x20}, 0}, {0x63, 1, {0xFF}, 0},
    {DISPON, 0, {0}, 0},
    {BRIGHTNESS, 1, {0xD0}, 0}, {0x58, 1, {0x00}, 0}, {0x36, 1, {0x00}, 10}, {INVOFF, 0, {0}, 0},
};

// The SH8601 (ESP32-S3-Touch-AMOLED-1.43's and the 1.8's chip): the pixel format and scan order first, as esp_lcd_sh8601 does,
// then Waveshare's list.
static const step_t SH8601_UP[] = {
    {0x36, 1, {0x00}, 0}, {0x3A, 1, {0x55}, 0},
    {SLPOUT, 0, {0}, 120},
    {0x44, 2, {0x01, 0xD1}, 0}, {0x35, 1, {0x00}, 0}, {0x53, 1, {0x20}, 10},
    {BRIGHTNESS, 1, {0x00}, 10}, {DISPON, 0, {0}, 10}, {BRIGHTNESS, 1, {0xD0}, 0},
};

// The RM690B0 (ESP32-S3-Touch-AMOLED-2.41), from Waveshare's example: page 0x20 settings, the SPI RAM write mode, then the usual.
static const step_t RM690B0_UP[] = {
    {0xFE, 1, {0x20}, 0}, {0x26, 1, {0x0A}, 0}, {0x24, 1, {0x80}, 0}, {0xFE, 1, {0x00}, 0},
    {0x3A, 1, {0x55}, 0}, {0xC2, 1, {0x00}, 10}, {0x35, 1, {0x00}, 0},
    {BRIGHTNESS, 1, {0x00}, 10},
    {SLPOUT, 0, {0}, 80},
    {DISPON, 0, {0}, 10}, {BRIGHTNESS, 1, {0xD0}, 0},
};

// Reads the controller's ID register on one data line before the SPI peripheral owns the pins, as Waveshare's firmware does
// to tell an SH8601 from a CO5300 on the same board: command 0x03 (read), the register, a dummy byte, then eight bits back.
// Bit-banged, so slow and simple; the pins are released again afterwards.
static void bang_bit(int sck, int d0, int level) {
    gpio_set_level(d0, level);
    gpio_set_level(sck, 0);
    gpio_set_level(sck, 1);
}

static void bang_byte(int sck, int d0, uint8_t byte) {
    for (int i = 0; i < 8; i++, byte <<= 1) bang_bit(sck, d0, (byte & 0x80) != 0);
}

static int read_id(const fusuma_amoled_config_t *cfg) {
    const uint64_t mask = (1ULL << cfg->cs) | (1ULL << cfg->sck) | (1ULL << cfg->d0) | (1ULL << cfg->rst);
    gpio_config_t out = {.pin_bit_mask = mask, .mode = GPIO_MODE_OUTPUT, .pull_up_en = GPIO_PULLUP_ENABLE};
    if (gpio_config(&out) != ESP_OK) return -1;
    gpio_set_level(cfg->rst, 1);
    sleep_ms(120);
    gpio_set_level(cfg->rst, 0);
    sleep_ms(120);
    gpio_set_level(cfg->rst, 1);
    sleep_ms(120);
    gpio_set_level(cfg->cs, 0);
    gpio_set_level(cfg->sck, 1);
    bang_byte(cfg->sck, cfg->d0, 0x03);
    bang_byte(cfg->sck, cfg->d0, 0x00);
    bang_byte(cfg->sck, cfg->d0, 0xDA);
    bang_byte(cfg->sck, cfg->d0, 0x00);
    uint8_t id = 0;
    for (int i = 0; i < 8; i++) {
        gpio_set_level(cfg->sck, 0);
        gpio_set_direction(cfg->d0, GPIO_MODE_INPUT);
        esp_rom_delay_us(1);
        id = (uint8_t)((id << 1) | gpio_get_level(cfg->d0));
        gpio_set_direction(cfg->d0, GPIO_MODE_OUTPUT);
        gpio_set_level(cfg->sck, 1);
        esp_rom_delay_us(1);
    }
    gpio_set_level(cfg->cs, 1);
    gpio_reset_pin(cfg->cs);
    gpio_reset_pin(cfg->sck);
    gpio_reset_pin(cfg->d0);
    return id;
}

esp_err_t fusuma_amoled_init(const fusuma_amoled_config_t *cfg) {
    int kind = cfg->kind;
    s_x_offset = cfg->x_offset;
    s_y_offset = cfg->y_offset;
    // A board that ships with either of two chips: ask the chip. Anything but the SH8601's ID is taken for a CO5300, as the
    // vendor does; the SH8601 has no column offset.
    if (kind == FUSUMA_AMOLED_CO5300 && cfg->probe && cfg->rst >= 0) {
        if (read_id(cfg) == SH8601_ID) {
            kind = FUSUMA_AMOLED_SH8601;
            s_x_offset = 0;
        }
    }
    esp_err_t err = ESP_OK;
    if (cfg->rst >= 0) {
        gpio_config_t rst = {.pin_bit_mask = 1ULL << cfg->rst, .mode = GPIO_MODE_OUTPUT};
        err = gpio_config(&rst);
        if (err != ESP_OK) return err;
    }

    spi_bus_config_t bus = {
        .sclk_io_num = cfg->sck,
        .data0_io_num = cfg->d0,
        .data1_io_num = cfg->d1,
        .data2_io_num = cfg->d2,
        .data3_io_num = cfg->d3,
        .max_transfer_sz = (int)cfg->max_transfer,
    };
    err = spi_bus_initialize(LCD_HOST, &bus, SPI_DMA_CH_AUTO);
    if (err != ESP_OK) return err;

    esp_lcd_panel_io_spi_config_t io = {
        .cs_gpio_num = cfg->cs,
        .dc_gpio_num = -1,
        .spi_mode = 0,
        .pclk_hz = cfg->pclk_hz,
        .trans_queue_depth = 4,
        .lcd_cmd_bits = 32,
        .lcd_param_bits = 8,
        .flags = {.quad_mode = true},
    };
    err = esp_lcd_new_panel_io_spi((esp_lcd_spi_bus_handle_t)LCD_HOST, &io, &s_io);
    if (err != ESP_OK) return err;

    // Reset (a pin, or the chip's own command when the board wires none), then the chip's sequence.
    if (cfg->rst >= 0) {
        gpio_set_level(cfg->rst, 1);
        sleep_ms(10);
        gpio_set_level(cfg->rst, 0);
        sleep_ms(200);
        gpio_set_level(cfg->rst, 1);
        sleep_ms(200);
    } else {
        err = write_reg(SWRESET, NULL, 0);
        if (err != ESP_OK) return err;
        sleep_ms(200);
    }
    switch (kind) {
    case FUSUMA_AMOLED_SH8601:
        return run(SH8601_UP, sizeof SH8601_UP / sizeof SH8601_UP[0]);
    case FUSUMA_AMOLED_RM690B0:
        return run(RM690B0_UP, sizeof RM690B0_UP / sizeof RM690B0_UP[0]);
    default:
        // The CO5300 wakes first, then takes its sequence.
        err = write_reg(SLPOUT, NULL, 0);
        if (err != ESP_OK) return err;
        sleep_ms(120);
        return run(CO5300_UP, sizeof CO5300_UP / sizeof CO5300_UP[0]);
    }
}

esp_err_t fusuma_amoled_draw(uint16_t x0, uint16_t y0, uint16_t x1, uint16_t y1, const uint16_t *pixels) {
    if (!s_io || x1 <= x0 || y1 <= y0) return ESP_ERR_INVALID_ARG;
    const uint16_t px0 = x0 + s_x_offset, px1 = x1 - 1 + s_x_offset;
    const uint16_t py0 = y0 + s_y_offset, py1 = y1 - 1 + s_y_offset;
    const uint8_t cols[4] = {px0 >> 8, px0 & 0xff, px1 >> 8, px1 & 0xff};
    const uint8_t rows[4] = {py0 >> 8, py0 & 0xff, py1 >> 8, py1 & 0xff};
    // A register write first waits for the pixel transfer before it, so the window never changes under it.
    esp_err_t err = write_reg(CASET, cols, 4);
    if (err != ESP_OK) return err;
    err = write_reg(PASET, rows, 4);
    if (err != ESP_OK) return err;
    // Start the memory write at the window's corner, or a partial update carries on where the last one stopped.
    err = write_reg(RAMWR, NULL, 0);
    if (err != ESP_OK) return err;
    const size_t bytes = (size_t)(x1 - x0) * (size_t)(y1 - y0) * 2;
    return esp_lcd_panel_io_tx_color(s_io, cmd(OP_WRITE_QUAD, RAMWR_CONTINUE), pixels, bytes);
}

esp_err_t fusuma_amoled_wait(void) {
    // A register write waits for every pixel transfer; this one is the panel's NOP command.
    return s_io ? esp_lcd_panel_io_tx_param(s_io, cmd(OP_WRITE, 0x00), NULL, 0) : ESP_ERR_INVALID_STATE;
}

esp_err_t fusuma_amoled_brightness(uint8_t level) {
    return s_io ? write_reg(BRIGHTNESS, &level, 1) : ESP_ERR_INVALID_STATE;
}
