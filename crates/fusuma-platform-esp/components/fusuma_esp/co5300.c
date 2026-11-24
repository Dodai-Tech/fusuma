// CO5300 AMOLED over QSPI. Command set and start-up sequence follow Hibari's Rust driver (hibari-drivers co5300),
// which follows Waveshare's Arduino_CO5300.
#include "fusuma_esp.h"

#include "driver/gpio.h"
#include "driver/spi_master.h"
#include "esp_lcd_panel_io.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define LCD_HOST SPI2_HOST

// QSPI opcodes: a register write on one line, a pixel write with the data on four.
#define OP_WRITE 0x02
#define OP_WRITE_QUAD 0x32

#define SLPOUT 0x11
#define INVOFF 0x20
#define DISPON 0x29
#define CASET 0x2A
#define PASET 0x2B
#define RAMWR 0x2C
#define RAMWR_CONTINUE 0x3C
#define BRIGHTNESS 0x51

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

esp_err_t fusuma_co5300_init(const fusuma_co5300_config_t *cfg) {
    s_x_offset = cfg->x_offset;
    s_y_offset = cfg->y_offset;
    gpio_config_t rst = {.pin_bit_mask = 1ULL << cfg->rst, .mode = GPIO_MODE_OUTPUT};
    esp_err_t err = gpio_config(&rst);
    if (err != ESP_OK) return err;

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

    // Reset, then wake.
    gpio_set_level(cfg->rst, 1);
    sleep_ms(10);
    gpio_set_level(cfg->rst, 0);
    sleep_ms(200);
    gpio_set_level(cfg->rst, 1);
    sleep_ms(200);
    err = write_reg(SLPOUT, NULL, 0);
    if (err != ESP_OK) return err;
    sleep_ms(120);

    // (register, value); the display goes on after the first five.
    static const uint8_t seq[][2] = {
        {0xFE, 0x00}, {0xC4, 0x80}, {0x3A, 0x55}, {0x53, 0x20}, {0x63, 0xFF},
        {BRIGHTNESS, 0xD0}, {0x58, 0x00}, {0x36, 0x00},
    };
    for (size_t i = 0; i < sizeof seq / sizeof seq[0]; i++) {
        if (i == 5) {
            err = write_reg(DISPON, NULL, 0);
            if (err != ESP_OK) return err;
        }
        err = write_reg(seq[i][0], &seq[i][1], 1);
        if (err != ESP_OK) return err;
    }
    sleep_ms(10);
    return write_reg(INVOFF, NULL, 0);
}

esp_err_t fusuma_co5300_draw(uint16_t x0, uint16_t y0, uint16_t x1, uint16_t y1, const uint16_t *pixels) {
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

esp_err_t fusuma_co5300_wait(void) {
    // A register write waits for every pixel transfer; this one is the panel's NOP command.
    return s_io ? esp_lcd_panel_io_tx_param(s_io, cmd(OP_WRITE, 0x00), NULL, 0) : ESP_ERR_INVALID_STATE;
}

esp_err_t fusuma_co5300_brightness(uint8_t level) {
    return s_io ? write_reg(BRIGHTNESS, &level, 1) : ESP_ERR_INVALID_STATE;
}
