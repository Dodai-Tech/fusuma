# The S3 watch's shim is only built for the S3; on another chip the component is empty, so one firmware crate can
# list every board's component.
if(NOT IDF_TARGET STREQUAL "esp32s3")
    idf_component_register()
    return()
endif()

idf_component_register(
    SRCS "s3_amoled.c"
    INCLUDE_DIRS "include"
    REQUIRES esp_lcd esp_driver_spi esp_driver_i2c esp_driver_gpio
)
