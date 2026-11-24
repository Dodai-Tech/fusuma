// A backlight on one LEDC PWM channel (timer 1, channel 1, 10-bit duty).
#include "fusuma_esp.h"

#include "driver/ledc.h"

static bool s_ready;

esp_err_t fusuma_ledc_backlight_init(int pin, bool invert, uint32_t freq_hz) {
    const ledc_timer_config_t timer = {
        .speed_mode = LEDC_LOW_SPEED_MODE,
        .duty_resolution = LEDC_TIMER_10_BIT,
        .timer_num = LEDC_TIMER_1,
        .freq_hz = freq_hz,
        .clk_cfg = LEDC_AUTO_CLK,
    };
    esp_err_t err = ledc_timer_config(&timer);
    if (err != ESP_OK) return err;
    const ledc_channel_config_t ch = {
        .gpio_num = pin,
        .speed_mode = LEDC_LOW_SPEED_MODE,
        .channel = LEDC_CHANNEL_1,
        .intr_type = LEDC_INTR_DISABLE,
        .timer_sel = LEDC_TIMER_1,
        .duty = 0,
        .hpoint = 0,
        .flags = {.output_invert = invert},
    };
    err = ledc_channel_config(&ch);
    s_ready = err == ESP_OK;
    return err;
}

void fusuma_ledc_backlight_set(int percent) {
    if (!s_ready) return;
    if (percent < 0) percent = 0;
    if (percent > 100) percent = 100;
    ledc_set_duty(LEDC_LOW_SPEED_MODE, LEDC_CHANNEL_1, (1023 * percent) / 100);
    ledc_update_duty(LEDC_LOW_SPEED_MODE, LEDC_CHANNEL_1);
}
