# Included after each project() in whisper.cpp's CMake build: whisper-rs-sys passes CMAKE_*
# variables from the environment, and .cargo/config.toml sets CMAKE_PROJECT_INCLUDE to this file.
#
# ggml builds its Vulkan shader generator as a nested ExternalProject deep inside Cargo's OUT_DIR,
# and there MSBuild's file tracker and MSVC's linker fail past Windows' 260-character path limit
# (FTK1011, LNK1104). The nested project gets a short base directory instead, one per build
# folder, under the workspace's target directory.
string(MD5 _kivo_build "${CMAKE_BINARY_DIR}")
string(SUBSTRING "${_kivo_build}" 0 10 _kivo_build)
set_property(DIRECTORY PROPERTY EP_BASE "${CMAKE_CURRENT_LIST_DIR}/../../target/ep/${_kivo_build}")
