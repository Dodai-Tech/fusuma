// E-paper over SPI: the bus, the data/command line, reset and busy. The controller's command sequences live in Rust
// (fusuma-platform-embedded, eink::ssd1677); this file only moves bytes and waits.
#include "fusuma_esp.h"

#include <string.h>

#include "driver/gpio.h"
#include "driver/spi_master.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_rom_sys.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

static const char *TAG = "epd";
#define CHUNK 4096

static spi_device_handle_t s_dev;
static int s_dc = -1, s_rst = -1, s_busy = -1, s_cs = -1, s_sck = -1, s_mosi = -1;
static uint32_t s_hz;
static uint8_t *s_dma;

esp_err_t fusuma_gpio_output(int pin, int level) {
    if (pin < 0) return ESP_OK;
    const gpio_config_t cfg = {
        .pin_bit_mask = 1ULL << pin,
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    esp_err_t err = gpio_config(&cfg);
    if (err != ESP_OK) return err;
    return gpio_set_level(pin, level);
}

esp_err_t fusuma_gpio_input_pullup(int pin) {
    if (pin < 0) return ESP_OK;
    const gpio_config_t cfg = {
        .pin_bit_mask = 1ULL << pin,
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_ENABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    return gpio_config(&cfg);
}

int fusuma_gpio_read(int pin) { return pin < 0 ? 0 : gpio_get_level(pin); }

esp_err_t fusuma_epd_init(const fusuma_epd_config_t *cfg) {
    s_dc = cfg->dc;
    s_rst = cfg->rst;
    s_busy = cfg->busy;
    s_cs = cfg->cs;
    s_sck = cfg->sck;
    s_mosi = cfg->mosi;
    s_hz = cfg->hz;
    // The panel's power first, then the lines it is controlled with.
    esp_err_t err = fusuma_gpio_output(cfg->en, 1);
    if (err != ESP_OK) return err;
    if ((err = fusuma_gpio_output(s_dc, 0)) != ESP_OK) return err;
    if ((err = fusuma_gpio_output(s_rst, 1)) != ESP_OK) return err;
    // The busy line is driven by the panel; a pull-down keeps it low (idle) if the panel is not there.
    const gpio_config_t busy = {
        .pin_bit_mask = 1ULL << s_busy,
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_ENABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    if ((err = gpio_config(&busy)) != ESP_OK) return err;

    const spi_bus_config_t bus = {
        .sclk_io_num = cfg->sck,
        .mosi_io_num = cfg->mosi,
        .miso_io_num = cfg->miso,
        .quadwp_io_num = -1,
        .quadhd_io_num = -1,
        .max_transfer_sz = CHUNK,
    };
    // The bus may be shared with an SD card on the same pins: a second init of the same host is "invalid state" and fine.
    err = spi_bus_initialize(SPI2_HOST, &bus, SPI_DMA_CH_AUTO);
    if (err != ESP_OK && err != ESP_ERR_INVALID_STATE) return err;
    const spi_device_interface_config_t dev = {
        .clock_speed_hz = (int)cfg->hz,
        .mode = 0,
        .spics_io_num = cfg->cs,
        .queue_size = 1,
    };
    if ((err = spi_bus_add_device(SPI2_HOST, &dev, &s_dev)) != ESP_OK) return err;
    s_dma = heap_caps_malloc(CHUNK, MALLOC_CAP_DMA | MALLOC_CAP_INTERNAL);
    if (!s_dma) return ESP_ERR_NO_MEM;
    ESP_LOGI(TAG, "SPI %u Hz, dc %d rst %d busy %d", (unsigned)cfg->hz, s_dc, s_rst, s_busy);
    return ESP_OK;
}

static esp_err_t send(const uint8_t *data, size_t len, int dc) {
    gpio_set_level(s_dc, dc);
    spi_transaction_t t = {.length = len * 8, .tx_buffer = data};
    return spi_device_polling_transmit(s_dev, &t);
}

esp_err_t fusuma_epd_command(uint8_t command) {
    s_dma[0] = command;
    return send(s_dma, 1, 0);
}

esp_err_t fusuma_epd_data(const uint8_t *data, size_t len) {
    while (len > 0) {
        size_t n = len < CHUNK ? len : CHUNK;
        // The frame is in PSRAM, which DMA cannot read: through an internal buffer, a piece at a time.
        memcpy(s_dma, data, n);
        esp_err_t err = send(s_dma, n, 1);
        if (err != ESP_OK) return err;
        data += n;
        len -= n;
    }
    return ESP_OK;
}

esp_err_t fusuma_epd_reset(void) {
    gpio_set_level(s_rst, 0);
    vTaskDelay(pdMS_TO_TICKS(10));
    gpio_set_level(s_rst, 1);
    vTaskDelay(pdMS_TO_TICKS(10));
    return ESP_OK;
}

esp_err_t fusuma_epd_wait_busy(uint32_t timeout_ms, int active_low) {
    TickType_t start = xTaskGetTickCount();
    // A short wait first: the line takes a moment to change after a command that starts work.
    vTaskDelay(pdMS_TO_TICKS(2));
    // Busy is high for the SSD16xx and low for the UC81xx.
    while ((gpio_get_level(s_busy) != 0) != (active_low != 0)) {
        if ((xTaskGetTickCount() - start) * portTICK_PERIOD_MS > timeout_ms) return ESP_ERR_TIMEOUT;
        vTaskDelay(pdMS_TO_TICKS(5));
    }
    return ESP_OK;
}

// SPI mode 0 at about 100 kHz: one byte now and then, and the controller's read timing is not documented well.
static void half(void) { esp_rom_delay_us(5); }

esp_err_t fusuma_epd_read_byte(uint8_t *value) {
    // Let the peripheral go of the pins, so they can be driven by hand.
    esp_err_t err = spi_bus_remove_device(s_dev);
    if (err != ESP_OK) return err;
    s_dev = NULL;
    spi_bus_free(SPI2_HOST);
    fusuma_gpio_output(s_sck, 0);
    fusuma_gpio_output(s_cs, 0);
    gpio_set_level(s_dc, 1);
    const gpio_config_t in = {
        .pin_bit_mask = 1ULL << s_mosi,
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    gpio_config(&in);
    half();
    uint8_t v = 0;
    for (int bit = 7; bit >= 0; bit--) {
        half();
        gpio_set_level(s_sck, 1);
        half();
        v = (uint8_t)((v << 1) | (gpio_get_level(s_mosi) ? 1 : 0));
        gpio_set_level(s_sck, 0);
    }
    gpio_set_level(s_cs, 1);
    *value = v;
    // The bus again, as fusuma_epd_init made it.
    const spi_bus_config_t bus = {
        .sclk_io_num = s_sck, .mosi_io_num = s_mosi, .miso_io_num = -1,
        .quadwp_io_num = -1, .quadhd_io_num = -1, .max_transfer_sz = CHUNK,
    };
    err = spi_bus_initialize(SPI2_HOST, &bus, SPI_DMA_CH_AUTO);
    if (err != ESP_OK) return err;
    const spi_device_interface_config_t dev = {
        .clock_speed_hz = (int)s_hz, .mode = 0, .spics_io_num = s_cs, .queue_size = 1,
    };
    return spi_bus_add_device(SPI2_HOST, &dev, &s_dev);
}

#include "driver/i2c_master.h"

esp_err_t fusuma_i2c_write_reg(int sda, int scl, int addr, int reg, int value, int mask) {
    i2c_master_bus_config_t bus_cfg = {
        .i2c_port = -1,
        .sda_io_num = sda,
        .scl_io_num = scl,
        .clk_source = I2C_CLK_SRC_DEFAULT,
        .glitch_ignore_cnt = 7,
        .flags.enable_internal_pullup = true,
    };
    i2c_master_bus_handle_t bus;
    esp_err_t err = i2c_new_master_bus(&bus_cfg, &bus);
    if (err != ESP_OK) return err;
    i2c_device_config_t dev = {
        .dev_addr_length = I2C_ADDR_BIT_LEN_7,
        .device_address = addr,
        .scl_speed_hz = 100000,
    };
    i2c_master_dev_handle_t h;
    err = i2c_master_bus_add_device(bus, &dev, &h);
    if (err == ESP_OK) {
        uint8_t out = (uint8_t)value;
        if (mask != 0xFF) {
            uint8_t r = (uint8_t)reg, old = 0;
            err = i2c_master_transmit_receive(h, &r, 1, &old, 1, 100);
            out = (uint8_t)((old & ~mask) | (value & mask));
        }
        uint8_t buf[2] = { (uint8_t)reg, out };
        if (err == ESP_OK) err = i2c_master_transmit(h, buf, sizeof buf, 100);
        i2c_master_bus_rm_device(h);
    }
    i2c_del_master_bus(bus);
    return err;
}
