// The two-controller e-paper panel (Waveshare's 13.3 inch six-colour): chip selects by hand, held across a command and its data, as
// Waveshare's ESP-IDF example does (`epaper_port.c`). The command sequences are in Rust (fusuma-platform-embedded, eink::epd13e6).
#include "fusuma_esp.h"

#include <string.h>

#include "driver/gpio.h"
#include "driver/spi_master.h"
#include "esp_heap_caps.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define CHUNK 4096

static spi_device_handle_t s_dev;
static int s_cs_m = -1, s_cs_s = -1, s_dc = -1, s_rst = -1, s_busy = -1, s_pwr = -1;
static uint8_t *s_dma;

esp_err_t fusuma_epd2_init(const fusuma_epd2_config_t *cfg) {
    s_cs_m = cfg->cs_master;
    s_cs_s = cfg->cs_slave;
    s_dc = cfg->dc;
    s_rst = cfg->rst;
    s_busy = cfg->busy;
    s_pwr = cfg->pwr;
    esp_err_t err;
    // Both chip selects idle high and the panel off until the driver powers it.
    if ((err = fusuma_gpio_output(s_cs_m, 1)) != ESP_OK) return err;
    if ((err = fusuma_gpio_output(s_cs_s, 1)) != ESP_OK) return err;
    if ((err = fusuma_gpio_output(s_dc, 0)) != ESP_OK) return err;
    if ((err = fusuma_gpio_output(s_rst, 1)) != ESP_OK) return err;
    if ((err = fusuma_gpio_output(s_pwr, 0)) != ESP_OK) return err;
    const gpio_config_t busy = {
        .pin_bit_mask = 1ULL << s_busy,
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    if ((err = gpio_config(&busy)) != ESP_OK) return err;

    const spi_bus_config_t bus = {
        .sclk_io_num = cfg->sck,
        .mosi_io_num = cfg->mosi,
        .miso_io_num = -1,
        .quadwp_io_num = -1,
        .quadhd_io_num = -1,
        .max_transfer_sz = CHUNK,
    };
    err = spi_bus_initialize(SPI3_HOST, &bus, SPI_DMA_CH_AUTO);
    if (err != ESP_OK && err != ESP_ERR_INVALID_STATE) return err;
    const spi_device_interface_config_t dev = {
        .clock_speed_hz = (int)cfg->hz,
        .mode = 0,
        // Both chip selects are driven by hand.
        .spics_io_num = -1,
        .queue_size = 1,
    };
    if ((err = spi_bus_add_device(SPI3_HOST, &dev, &s_dev)) != ESP_OK) return err;
    s_dma = heap_caps_malloc(CHUNK, MALLOC_CAP_DMA | MALLOC_CAP_INTERNAL);
    return s_dma ? ESP_OK : ESP_ERR_NO_MEM;
}

esp_err_t fusuma_epd2_power(int on) {
    // Power off also parks reset low, as the vendor's sleep does.
    if (!on) gpio_set_level(s_rst, 0);
    return fusuma_gpio_output(s_pwr, on ? 1 : 0);
}

esp_err_t fusuma_epd2_reset(void) {
    for (int i = 0; i < 5; i++) {
        gpio_set_level(s_rst, (i % 2) == 0);
        vTaskDelay(pdMS_TO_TICKS(30));
    }
    return ESP_OK;
}

static esp_err_t send(const uint8_t *data, size_t len, int dc) {
    gpio_set_level(s_dc, dc);
    spi_transaction_t t = {.length = len * 8, .tx_buffer = data};
    return spi_device_polling_transmit(s_dev, &t);
}

esp_err_t fusuma_epd2_begin(int chips, uint8_t command) {
    gpio_set_level(s_cs_m, (chips & 1) ? 0 : 1);
    gpio_set_level(s_cs_s, (chips & 2) ? 0 : 1);
    s_dma[0] = command;
    return send(s_dma, 1, 0);
}

esp_err_t fusuma_epd2_data(const uint8_t *data, size_t len) {
    while (len > 0) {
        size_t n = len < CHUNK ? len : CHUNK;
        // The picture is in PSRAM, which DMA cannot read: through an internal buffer, a piece at a time.
        memcpy(s_dma, data, n);
        esp_err_t err = send(s_dma, n, 1);
        if (err != ESP_OK) return err;
        data += n;
        len -= n;
    }
    return ESP_OK;
}

esp_err_t fusuma_epd2_end(void) {
    gpio_set_level(s_cs_m, 1);
    gpio_set_level(s_cs_s, 1);
    return ESP_OK;
}

esp_err_t fusuma_epd2_wait_idle(uint32_t timeout_ms) {
    TickType_t start = xTaskGetTickCount();
    // The line takes a moment to change after a command that starts work.
    vTaskDelay(pdMS_TO_TICKS(2));
    while (gpio_get_level(s_busy) == 0) {
        if ((xTaskGetTickCount() - start) * portTICK_PERIOD_MS > timeout_ms) return ESP_ERR_TIMEOUT;
        vTaskDelay(pdMS_TO_TICKS(10));
    }
    return ESP_OK;
}
