cmake_minimum_required(VERSION 3.21)

set(PRODUCT_VERSION_MAJOR 1)
set(PRODUCT_VERSION_MINOR 1)
set(PRODUCT_VERSION_PATCH 0)

# TSDL_USE_VCPKG: dependencies come from the vcpkg manifest (toolchain set by the presets).
# TSDL_SYSTEM_LIBTORRENT: link an installed libtorrent-rasterbar instead (distro / Flatpak builds, no network).
option(TSDL_USE_VCPKG "Resolve dependencies through vcpkg" ON)
option(TSDL_SYSTEM_LIBTORRENT "Use an installed libtorrent-rasterbar (CMake config or pkg-config)" OFF)
set(TSDL_INSTALL_DIR "." CACHE STRING "Install folder of the library, relative to the prefix")

if(TSDL_SYSTEM_LIBTORRENT)
  set(VCPKG_MANIFEST_MODE OFF CACHE BOOL "" FORCE)
endif()

if(CMAKE_SYSTEM_NAME MATCHES "Darwin")
  set(CMAKE_OSX_DEPLOYMENT_TARGET 11 CACHE STRING "" FORCE)
endif()

if(POLICY CMP0167)
  cmake_policy(SET CMP0167 NEW)
endif()

project(TorrentSharp LANGUAGES CXX VERSION ${PRODUCT_VERSION_MAJOR}.${PRODUCT_VERSION_MINOR}.${PRODUCT_VERSION_PATCH})

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(CMAKE_CXX_VISIBILITY_PRESET hidden)
set(CMAKE_VISIBILITY_INLINES_HIDDEN ON)

find_package(LibtorrentRasterbar 2.0 CONFIG QUIET)
if(NOT LibtorrentRasterbar_FOUND)
  if(NOT TSDL_SYSTEM_LIBTORRENT)
    message(FATAL_ERROR "LibtorrentRasterbar not found. Configure with a vcpkg preset, or pass -DTSDL_SYSTEM_LIBTORRENT=ON.")
  endif()
  find_package(PkgConfig REQUIRED)
  pkg_check_modules(LIBTORRENT REQUIRED IMPORTED_TARGET libtorrent-rasterbar>=2.0)
  add_library(LibtorrentRasterbar::torrent-rasterbar ALIAS PkgConfig::LIBTORRENT)
endif()

add_library(tsdl SHARED
  TorrentSharp/src/TorrentSharp.cpp
  TorrentSharp/include/TorrentSharp.h)

target_include_directories(tsdl PRIVATE TorrentSharp/include)
target_compile_definitions(tsdl PRIVATE TSDL_BUILDING
  TSDL_VERSION_STRING="${PROJECT_VERSION}")
target_link_libraries(tsdl PRIVATE LibtorrentRasterbar::torrent-rasterbar)

if(CMAKE_SYSTEM_NAME MATCHES "Linux")
  # One self-contained .so: only the tsdl_* C API is exported, the static deps and
  # libstdc++/libgcc stay inside, and the result depends on glibc alone.
  set(TSDL_EXPORTS_MAP ${CMAKE_CURRENT_SOURCE_DIR}/TorrentSharp/exports.map)
  set_target_properties(tsdl PROPERTIES LINK_DEPENDS ${TSDL_EXPORTS_MAP})
  target_link_options(tsdl PRIVATE
    -static-libstdc++ -static-libgcc
    -Wl,--version-script=${TSDL_EXPORTS_MAP}
    -Wl,--exclude-libs,ALL
    -Wl,--gc-sections
    -Wl,--as-needed
    -Wl,-z,defs
    -Wl,-z,relro,-z,now)
  target_compile_options(tsdl PRIVATE -ffunction-sections -fdata-sections)
  set_target_properties(tsdl PROPERTIES INSTALL_RPATH "$ORIGIN" BUILD_WITH_INSTALL_RPATH ON)
elseif(CMAKE_SYSTEM_NAME MATCHES "Darwin")
  set_target_properties(tsdl PROPERTIES INSTALL_RPATH "@loader_path" BUILD_WITH_INSTALL_RPATH ON)
elseif(MSVC)
  target_compile_options(tsdl PRIVATE /utf-8 /EHsc)
endif()

# Installs straight into a runtimes/<rid>/native folder: `cmake --install <build> --strip --prefix <dir>`.
install(TARGETS tsdl
  LIBRARY DESTINATION ${TSDL_INSTALL_DIR}
  RUNTIME DESTINATION ${TSDL_INSTALL_DIR}
  ARCHIVE EXCLUDE_FROM_ALL)
