cmake_minimum_required(VERSION 3.14)
project(llm_server)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Build llama.cpp as static library
set(LLAMA_METAL ON CACHE BOOL "Enable Metal support" FORCE)
set(LLAMA_METAL_EMBED_LIBRARY ON CACHE BOOL "Embed Metal library" FORCE)
set(LLAMA_STATIC ON CACHE BOOL "Build static library" FORCE)

# Add llama.cpp subdirectory
add_subdirectory(llama.cpp)

# Create our wrapper library
add_library(llm_server STATIC llm_server.cpp llm_server.h)

target_include_directories(llm_server PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/llama.cpp/include
    ${CMAKE_CURRENT_SOURCE_DIR}/llama.cpp/ggml/include
)

target_link_libraries(llm_server PRIVATE
    llama
    ggml
    ggml-metal
    ggml-blas
    ggml-cpu
    ggml-base
)

if(APPLE)
    find_library(METAL_FRAMEWORK Metal)
    find_library(FOUNDATION_FRAMEWORK Foundation)
    find_library(ACCELERATE_FRAMEWORK Accelerate)
    target_link_libraries(llm_server PRIVATE
        ${METAL_FRAMEWORK}
        ${FOUNDATION_FRAMEWORK}
        ${ACCELERATE_FRAMEWORK}
    )
endif()
