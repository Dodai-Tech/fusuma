# The P4 board's shim is only built for the P4; on another chip the component is empty, so one firmware crate can
# list every board's component.
if(NOT IDF_TARGET STREQUAL "esp32p4")
    idf_component_register()
    return()
endif()

idf_component_register(
    SRCS "wt_display.c"
    INCLUDE_DIRS "include"
    REQUIRES esp_lcd esp_driver_ledc esp_driver_i2c esp_driver_gpio esp_mm
)
