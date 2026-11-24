// GT911 touch over I2C, through esp_lcd_touch_gt911 on the new i2c_master driver. Orientation (mirror, swap) is
// applied by the caller from the board file, so the controller reports as it is mounted.
#include "fusuma_esp.h"

#include "driver/i2c_master.h"
#include "esp_lcd_panel_io.h"
#include "esp_lcd_touch.h"
#include "esp_lcd_touch_gt911.h"
#include "esp_log.h"

static const char *TAG = "gt911";
static esp_lcd_touch_handle_t s_touch;

esp_err_t fusuma_gt911_init(const fusuma_touch_config_t *cfg) {
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

    // The GT911 answers at 0x5D or 0x14, depending on the INT and RESET levels at power-up.
    const uint8_t candidates[2] = {ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS, ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS_BACKUP};
    uint8_t found = 0;
    for (int i = 0; i < 2 && !found; i++) {
        if (i2c_master_probe(bus, candidates[i], 100) == ESP_OK) found = candidates[i];
    }
    if (!found) {
        ESP_LOGE(TAG, "no GT911 at 0x%02x or 0x%02x", candidates[0], candidates[1]);
        return ESP_ERR_NOT_FOUND;
    }
    ESP_LOGI(TAG, "GT911 at 0x%02x", found);

    esp_lcd_panel_io_i2c_config_t io_cfg = ESP_LCD_TOUCH_IO_I2C_GT911_CONFIG();
    io_cfg.dev_addr = found;
    io_cfg.scl_speed_hz = cfg->hz;
    esp_lcd_panel_io_handle_t io = NULL;
    err = esp_lcd_new_panel_io_i2c_v2(bus, &io_cfg, &io);
    if (err != ESP_OK) return err;

    const esp_lcd_touch_config_t tp_cfg = {
        .x_max = cfg->width,
        .y_max = cfg->height,
        .rst_gpio_num = cfg->rst,
        .int_gpio_num = cfg->irq,
        .levels = {.reset = 0, .interrupt = 0},
        .flags = {.swap_xy = 0, .mirror_x = 0, .mirror_y = 0},
    };
    return esp_lcd_touch_new_i2c_gt911(io, &tp_cfg, &s_touch);
}

bool fusuma_gt911_read(bool *down, uint16_t *x, uint16_t *y) {
    if (!s_touch) return false;
    if (esp_lcd_touch_read_data(s_touch) != ESP_OK) return false;
    uint16_t tx[1], ty[1];
    uint8_t cnt = 0;
    *down = esp_lcd_touch_get_coordinates(s_touch, tx, ty, NULL, &cnt, 1) && cnt > 0;
    if (*down) {
        *x = tx[0];
        *y = ty[0];
    }
    return true;
}
