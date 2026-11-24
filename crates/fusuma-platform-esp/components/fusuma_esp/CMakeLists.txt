# Fusuma's ESP drivers, one file per controller. Which ones a firmware calls comes from its board file; the rest are
# left out by the linker. The DSI panel needs the P4's MIPI DSI, so it is only built there.
set(srcs "co5300.c" "ft3168.c" "gt911.c" "ledc_backlight.c")
set(reqs esp_lcd esp_driver_spi esp_driver_i2c esp_driver_gpio esp_driver_ledc)
if(IDF_TARGET STREQUAL "esp32p4")
    list(APPEND srcs "ek79007.c")
    list(APPEND reqs esp_mm esp_hw_support)
endif()

idf_component_register(
    SRCS ${srcs}
    INCLUDE_DIRS "include"
    REQUIRES ${reqs}
)
