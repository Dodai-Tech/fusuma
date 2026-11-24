// EK79007 1024x600 MIPI DSI panel (ESP32-P4). Timings from Waveshare's BSP (waveshare/esp32_p4_wifi6_touch_lcd_7b
// 3.0.1); the frame buffer is scanned out by DMA, so the CPU writes pixels into it and flushes the cache.
#include "fusuma_esp.h"

#include <string.h>

#include "esp_cache.h"
#include "esp_check.h"
#include "esp_lcd_ek79007.h"
#include "esp_lcd_mipi_dsi.h"
#include "esp_lcd_panel_io.h"
#include "esp_lcd_panel_ops.h"
#include "esp_ldo_regulator.h"
#include "esp_log.h"

#define W 1024
#define H 600

static const char *TAG = "ek79007";
static esp_lcd_panel_handle_t s_panel;
static uint16_t *s_fb;

esp_err_t fusuma_ek79007_init(const fusuma_ek79007_config_t *cfg) {
    if (s_fb) return ESP_OK;
    if (cfg->phy_ldo_channel >= 0) {
        // The DSI PHY's supply.
        static esp_ldo_channel_handle_t phy_ldo;
        const esp_ldo_channel_config_t ldo_cfg = {
            .chan_id = cfg->phy_ldo_channel,
            .voltage_mv = cfg->phy_ldo_mv,
        };
        ESP_RETURN_ON_ERROR(esp_ldo_acquire_channel(&ldo_cfg, &phy_ldo), TAG, "dphy ldo");
    }
    esp_lcd_dsi_bus_handle_t dsi_bus = NULL;
    const esp_lcd_dsi_bus_config_t bus_cfg = {
        .bus_id = 0,
        .num_data_lanes = cfg->lanes,
        .phy_clk_src = 0,
        .lane_bit_rate_mbps = cfg->lane_mbps,
    };
    ESP_RETURN_ON_ERROR(esp_lcd_new_dsi_bus(&bus_cfg, &dsi_bus), TAG, "dsi bus");

    esp_lcd_panel_io_handle_t io = NULL;
    const esp_lcd_dbi_io_config_t dbi_cfg = {.virtual_channel = 0, .lcd_cmd_bits = 8, .lcd_param_bits = 8};
    ESP_RETURN_ON_ERROR(esp_lcd_new_panel_io_dbi(dsi_bus, &dbi_cfg, &io), TAG, "dbi io");

    esp_lcd_dpi_panel_config_t dpi_cfg = EK79007_1024_600_PANEL_60HZ_CONFIG(LCD_COLOR_PIXEL_FORMAT_RGB565);
    dpi_cfg.num_fbs = 1;
    ek79007_vendor_config_t vendor_cfg = {.mipi_config = {.dsi_bus = dsi_bus, .dpi_config = &dpi_cfg}};
    const esp_lcd_panel_dev_config_t panel_cfg = {
        .bits_per_pixel = 16,
        .rgb_ele_order = LCD_RGB_ELEMENT_ORDER_RGB,
        .reset_gpio_num = cfg->rst,
        .vendor_config = &vendor_cfg,
    };
    ESP_RETURN_ON_ERROR(esp_lcd_new_panel_ek79007(io, &panel_cfg, &s_panel), TAG, "ek79007");
    ESP_RETURN_ON_ERROR(esp_lcd_panel_reset(s_panel), TAG, "panel reset");
    ESP_RETURN_ON_ERROR(esp_lcd_panel_init(s_panel), TAG, "panel init");

    void *fb = NULL;
    ESP_RETURN_ON_ERROR(esp_lcd_dpi_panel_get_frame_buffer(s_panel, 1, &fb), TAG, "get fb");
    s_fb = (uint16_t *)fb;
    memset(s_fb, 0, W * H * sizeof(uint16_t));
    fusuma_ek79007_flush_rows(0, H);
    ESP_LOGI(TAG, "display ready %dx%d", W, H);
    return ESP_OK;
}

uint16_t *fusuma_ek79007_framebuffer(void) {
    return s_fb;
}

esp_err_t fusuma_ek79007_flush_rows(uint32_t y0, uint32_t y1) {
    if (!s_fb || y1 <= y0) return ESP_OK;
    if (y1 > H) y1 = H;
    uint8_t *start = (uint8_t *)s_fb + (size_t)y0 * W * sizeof(uint16_t);
    size_t size = (size_t)(y1 - y0) * W * sizeof(uint16_t);
    return esp_cache_msync(start, size, ESP_CACHE_MSYNC_FLAG_DIR_C2M | ESP_CACHE_MSYNC_FLAG_UNALIGNED);
}
