// CST9217 / CST9220 touch over I2C, address 0x5A, 16-bit registers. The report is read from 0xD000 and acknowledged by writing
// 0xAB back to it; follows Waveshare's SensorLib (TouchDrvCST92xx), read, not run on a board.
#include "fusuma_esp.h"

#include "driver/gpio.h"
#include "driver/i2c_master.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define ADDRESS 0x5A
#define REG_REPORT 0xD000
#define ACK 0xAB
// The report: two fingers of five bytes, then five bytes of header, so 15 bytes; byte 6 holds the ACK, byte 5 the finger count.
#define REPORT_LEN 15
#define PRESSED 0x06

static i2c_master_dev_handle_t s_dev;

esp_err_t fusuma_cst92xx_init(const fusuma_touch_config_t *cfg) {
    if (cfg->rst >= 0) {
        gpio_config_t rst = {.pin_bit_mask = 1ULL << cfg->rst, .mode = GPIO_MODE_OUTPUT};
        esp_err_t err = gpio_config(&rst);
        if (err != ESP_OK) return err;
        gpio_set_level(cfg->rst, 0);
        vTaskDelay(pdMS_TO_TICKS(10));
        gpio_set_level(cfg->rst, 1);
        vTaskDelay(pdMS_TO_TICKS(50));
    }
    i2c_master_bus_config_t bus_cfg = {
        .i2c_port = -1,
        .sda_io_num = cfg->sda,
        .scl_io_num = cfg->scl,
        .clk_source = I2C_CLK_SRC_DEFAULT,
        .glitch_ignore_cnt = 7,
        .flags.enable_internal_pullup = true,
    };
    i2c_master_bus_handle_t bus;
    esp_err_t err = i2c_new_master_bus(&bus_cfg, &bus);
    if (err != ESP_OK) return err;
    i2c_device_config_t dev = {
        .dev_addr_length = I2C_ADDR_BIT_LEN_7,
        .device_address = ADDRESS,
        .scl_speed_hz = cfg->hz,
    };
    err = i2c_master_bus_add_device(bus, &dev, &s_dev);
    if (err != ESP_OK) return err;
    return i2c_master_probe(bus, ADDRESS, 50);
}

bool fusuma_cst92xx_read(bool *down, uint16_t *x, uint16_t *y) {
    if (!s_dev) return false;
    const uint8_t reg[2] = {REG_REPORT >> 8, REG_REPORT & 0xFF};
    uint8_t raw[REPORT_LEN] = {0};
    if (i2c_master_transmit_receive(s_dev, reg, sizeof reg, raw, sizeof raw, 20) != ESP_OK) return false;
    const uint8_t ack[3] = {REG_REPORT >> 8, REG_REPORT & 0xFF, ACK};
    if (i2c_master_transmit(s_dev, ack, sizeof ack, 20) != ESP_OK) return false;
    // No ACK back: no report to read (not an error, the chip was not ready).
    const uint8_t fingers = raw[5] & 0x7F;
    *down = raw[6] == ACK && fingers > 0 && fingers <= 2 && (raw[0] & 0x0F) == PRESSED;
    if (*down) {
        *x = (uint16_t)((raw[1] << 4) | (raw[3] >> 4));
        *y = (uint16_t)((raw[2] << 4) | (raw[3] & 0x0F));
    }
    return true;
}
