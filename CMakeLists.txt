cmake_minimum_required(VERSION 3.21)

# ── Project ──────────────────────────────────────────────────────────────────
set(TARGET_NAME financial_functions)
project(${TARGET_NAME})

# ── DuckDB extension infrastructure ──────────────────────────────────────────
# Pull in the standard DuckDB extension cmake helpers.
# The CI toolchain provides duckdb_extension_load() and the build variables.
find_package(DuckDB REQUIRED)

# ── Rust build via Cargo ──────────────────────────────────────────────────────
# Locate cargo (required; the CI runners have Rust pre-installed).
find_program(CARGO cargo REQUIRED)

# Build mode follows the CMake build type.
if(CMAKE_BUILD_TYPE STREQUAL "Release")
  set(CARGO_BUILD_FLAG "--release")
  set(CARGO_TARGET_DIR "${CMAKE_CURRENT_BINARY_DIR}/target/release")
else()
  set(CARGO_BUILD_FLAG "")
  set(CARGO_TARGET_DIR "${CMAKE_CURRENT_BINARY_DIR}/target/debug")
endif()

# The Rust crate produces a cdylib named after [package] name in Cargo.toml.
# On Linux/macOS it is libfinancial_functions.so / .dylib;
# on Windows it is financial_functions.dll.
# DuckDB's loader renames it to financial_functions.duckdb_extension.
add_custom_target(
  cargo_build ALL
  COMMAND ${CARGO} build ${CARGO_BUILD_FLAG}
          --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
          --target-dir    "${CMAKE_CURRENT_BINARY_DIR}/target"
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
  COMMENT "Building Rust crate with Cargo…"
)

# ── Expose the built cdylib to DuckDB's extension machinery ──────────────────
# Determine the platform-specific shared library name.
if(WIN32)
  set(LIB_NAME "${CARGO_TARGET_DIR}/financial_functions.dll")
elseif(APPLE)
  set(LIB_NAME "${CARGO_TARGET_DIR}/libfinancial_functions.dylib")
else()
  set(LIB_NAME "${CARGO_TARGET_DIR}/libfinancial_functions.so")
endif()

# Import the Cargo output as a pre-built shared library so that the DuckDB
# cmake helpers can sign and package it correctly.
add_library(${TARGET_NAME} SHARED IMPORTED GLOBAL)
set_target_properties(${TARGET_NAME} PROPERTIES
  IMPORTED_LOCATION "${LIB_NAME}"
)
add_dependencies(${TARGET_NAME} cargo_build)

# Register with the DuckDB extension framework.
duckdb_extension_load(${TARGET_NAME}
  DONT_LINK          # Rust produces its own cdylib; no C++ link step needed
  LOAD_TESTS
  GIT_TAG            ""
  LINKED_LIBS        ""
)
