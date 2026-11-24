// See include/s3_amoled.h.
#include "s3_amoled.h"

#include "driver/gpio.h"
#include "driver/i2c_master.h"
#include "driver/spi_master.h"
#include "esp_lcd_panel_io.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

static const char *TAG = "s3_amoled";

// Pins of the board (Hibari's boards/s3-amoled-206).
#define PIN_LCD_SCK 11
#define PIN_LCD_D0 4
#define PIN_LCD_D1 5
#define PIN_LCD_D2 6
#define PIN_LCD_D3 7
#define PIN_LCD_CS 12
#define PIN_LCD_RST 8
#define PIN_TP_SDA 15
#define PIN_TP_SCL 14
#define PIN_TP_RST 9

#define LCD_HOST SPI2_HOST
#define LCD_PCLK_HZ (80 * 1000 * 1000)
// The panel's memory starts 22 columns in.
#define LCD_X_OFFSET 22
// The largest pixel transfer: one band of rows, as the Rust side sends them.
#define LCD_MAX_TRANSFER (32 * 1024)

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

#define TP_ADDRESS 0x38
#define TP_REG_FINGERS 0x02
#define TP_REG_POWER_MODE 0xA5
#define TP_POWER_MONITOR 0x01

static esp_lcd_panel_io_handle_t s_io;
static i2c_master_dev_handle_t s_touch;

static int cmd(uint8_t opcode, uint8_t reg) {
    return (int)(((uint32_t)opcode << 24) | ((uint32_t)reg << 8));
}

static esp_err_t write_reg(uint8_t reg, const uint8_t *data, size_t len) {
    return esp_lcd_panel_io_tx_param(s_io, cmd(OP_WRITE, reg), data, len);
}

static void sleep_ms(uint32_t ms) {
    vTaskDelay(pdMS_TO_TICKS(ms) ? pdMS_TO_TICKS(ms) : 1);
}

static esp_err_t panel_init(void) {
    gpio_config_t rst = {.pin_bit_mask = 1ULL << PIN_LCD_RST, .mode = GPIO_MODE_OUTPUT};
    esp_err_t err = gpio_config(&rst);
    if (err != ESP_OK) return err;

    spi_bus_config_t bus = {
        .sclk_io_num = PIN_LCD_SCK,
        .data0_io_num = PIN_LCD_D0,
        .data1_io_num = PIN_LCD_D1,
        .data2_io_num = PIN_LCD_D2,
        .data3_io_num = PIN_LCD_D3,
        .max_transfer_sz = LCD_MAX_TRANSFER,
    };
    err = spi_bus_initialize(LCD_HOST, &bus, SPI_DMA_CH_AUTO);
    if (err != ESP_OK) return err;

    esp_lcd_panel_io_spi_config_t io = {
        .cs_gpio_num = PIN_LCD_CS,
        .dc_gpio_num = -1,
        .spi_mode = 0,
        .pclk_hz = LCD_PCLK_HZ,
        .trans_queue_depth = 4,
        .lcd_cmd_bits = 32,
        .lcd_param_bits = 8,
        .flags = {.quad_mode = true},
    };
    err = esp_lcd_new_panel_io_spi((esp_lcd_spi_bus_handle_t)LCD_HOST, &io, &s_io);
    if (err != ESP_OK) return err;

    // Reset, then wake, as the Rust driver does.
    gpio_set_level(PIN_LCD_RST, 1);
    sleep_ms(10);
    gpio_set_level(PIN_LCD_RST, 0);
    sleep_ms(200);
    gpio_set_level(PIN_LCD_RST, 1);
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

static esp_err_t touch_init(void) {
    gpio_config_t rst = {.pin_bit_mask = 1ULL << PIN_TP_RST, .mode = GPIO_MODE_OUTPUT};
    esp_err_t err = gpio_config(&rst);
    if (err != ESP_OK) return err;
    gpio_set_level(PIN_TP_RST, 0);
    sleep_ms(10);
    gpio_set_level(PIN_TP_RST, 1);
    sleep_ms(50);

    i2c_master_bus_config_t bus_cfg = {
        .i2c_port = I2C_NUM_0,
        .sda_io_num = PIN_TP_SDA,
        .scl_io_num = PIN_TP_SCL,
        .clk_source = I2C_CLK_SRC_DEFAULT,
        .glitch_ignore_cnt = 7,
        .flags.enable_internal_pullup = true,
    };
    i2c_master_bus_handle_t bus;
    err = i2c_new_master_bus(&bus_cfg, &bus);
    if (err != ESP_OK) return err;
    i2c_device_config_t dev = {
        .dev_addr_length = I2C_ADDR_BIT_LEN_7,
        .device_address = TP_ADDRESS,
        .scl_speed_hz = 400000,
    };
    err = i2c_master_bus_add_device(bus, &dev, &s_touch);
    if (err != ESP_OK) return err;
    // Monitor power mode: the controller wakes on a touch.
    const uint8_t mode[2] = {TP_REG_POWER_MODE, TP_POWER_MONITOR};
    return i2c_master_transmit(s_touch, mode, sizeof mode, 50);
}

esp_err_t s3_amoled_init(void) {
    esp_err_t err = panel_init();
    if (err != ESP_OK) return err;
    err = touch_init();
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "touch init failed (%s); the panel works without it", esp_err_to_name(err));
    }
    return ESP_OK;
}

esp_err_t s3_amoled_draw(uint16_t x0, uint16_t y0, uint16_t x1, uint16_t y1, const uint16_t *pixels) {
    if (x1 <= x0 || y1 <= y0 || x1 > S3_LCD_W || y1 > S3_LCD_H || ((x0 | y0 | x1 | y1) & 1)) {
        return ESP_ERR_INVALID_ARG;
    }
    const uint16_t px0 = x0 + LCD_X_OFFSET, px1 = x1 - 1 + LCD_X_OFFSET, py1 = y1 - 1;
    const uint8_t cols[4] = {px0 >> 8, px0 & 0xff, px1 >> 8, px1 & 0xff};
    const uint8_t rows[4] = {y0 >> 8, y0 & 0xff, py1 >> 8, py1 & 0xff};
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

esp_err_t s3_amoled_wait(void) {
    // A register write waits for every pixel transfer; this one is the panel's NOP command.
    return esp_lcd_panel_io_tx_param(s_io, cmd(OP_WRITE, 0x00), NULL, 0);
}

esp_err_t s3_amoled_brightness(uint8_t level) {
    return write_reg(BRIGHTNESS, &level, 1);
}

bool s3_touch_read(bool *down, uint16_t *x, uint16_t *y) {
    if (!s_touch) return false;
    const uint8_t reg = TP_REG_FINGERS;
    uint8_t raw[5] = {0};
    if (i2c_master_transmit_receive(s_touch, &reg, 1, raw, sizeof raw, 20) != ESP_OK) {
        return false;
    }
    *down = raw[0] != 0;
    // The top four bits of each high byte are event flags, not position.
    *x = (uint16_t)(((raw[1] & 0x0F) << 8) | raw[2]);
    *y = (uint16_t)(((raw[3] & 0x0F) << 8) | raw[4]);
    return true;
}
