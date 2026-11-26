// Touch over I2C for the chips that share FocalTech's register map: the FT3168 (Hibari's hibari-drivers ft3168), the FT5x06 and the
// CST816, which differ in their address and in whether they take the power-mode register.
#include "fusuma_esp.h"

#include "driver/gpio.h"
#include "driver/i2c_master.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define ADDRESS_FT 0x38
#define ADDRESS_CST816 0x15
#define REG_FINGERS 0x02
#define REG_POWER_MODE 0xA5
#define POWER_MONITOR 0x01

static i2c_master_dev_handle_t s_dev;

static esp_err_t init(const fusuma_touch_config_t *cfg, uint8_t address, bool power_mode) {
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
        .device_address = address,
        .scl_speed_hz = cfg->hz,
    };
    err = i2c_master_bus_add_device(bus, &dev, &s_dev);
    if (err != ESP_OK) return err;
    if (!power_mode) {
        // Prove the chip is there: a read of the finger register.
        const uint8_t reg = REG_FINGERS;
        uint8_t one;
        return i2c_master_transmit_receive(s_dev, &reg, 1, &one, 1, 50);
    }
    // Monitor power mode: the controller wakes on a touch.
    const uint8_t mode[2] = {REG_POWER_MODE, POWER_MONITOR};
    return i2c_master_transmit(s_dev, mode, sizeof mode, 50);
}

esp_err_t fusuma_ft3168_init(const fusuma_touch_config_t *cfg) { return init(cfg, ADDRESS_FT, true); }
esp_err_t fusuma_ft5x06_init(const fusuma_touch_config_t *cfg) { return init(cfg, ADDRESS_FT, false); }
esp_err_t fusuma_cst816_init(const fusuma_touch_config_t *cfg) { return init(cfg, ADDRESS_CST816, false); }

bool fusuma_ft3168_read(bool *down, uint16_t *x, uint16_t *y) {
    if (!s_dev) return false;
    const uint8_t reg = REG_FINGERS;
    uint8_t raw[5] = {0};
    if (i2c_master_transmit_receive(s_dev, &reg, 1, raw, sizeof raw, 20) != ESP_OK) return false;
    *down = raw[0] != 0;
    // The top four bits of each high byte are event flags, not position.
    *x = (uint16_t)(((raw[1] & 0x0F) << 8) | raw[2]);
    *y = (uint16_t)(((raw[3] & 0x0F) << 8) | raw[4]);
    return true;
}
