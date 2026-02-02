cmake_minimum_required(VERSION 3.20)
project(cemu_adapter C)
set(CMAKE_C_STANDARD 11)
set(CEMU_CORE_DIR "${CMAKE_SOURCE_DIR}/../../cemu-ref/core")
file(GLOB CEMU_SOURCES "${CEMU_CORE_DIR}/*.c" "${CEMU_CORE_DIR}/usb/*.c")
list(APPEND CEMU_SOURCES "${CEMU_CORE_DIR}/os/os-linux.c")
set(ADAPTER_SOURCE "${CMAKE_SOURCE_DIR}/../../android/app/src/main/cpp/cemu/cemu_adapter.c")
add_library(cemu_adapter STATIC ${CEMU_SOURCES} ${ADAPTER_SOURCE})
target_include_directories(cemu_adapter PRIVATE
    ${CEMU_CORE_DIR}
    ${CEMU_CORE_DIR}/usb
    ${CEMU_CORE_DIR}/os
    ${CMAKE_SOURCE_DIR}/../include
)
target_compile_definitions(cemu_adapter PRIVATE MULTITHREAD=0 CEMU_NO_UI=1)
target_compile_options(cemu_adapter PRIVATE -w)
