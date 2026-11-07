# Static deps for a self-contained libtsdl.so: everything is linked into tsdl,
# so it must be position independent and keep its symbols hidden.
set(VCPKG_TARGET_ARCHITECTURE x64)
set(VCPKG_CRT_LINKAGE dynamic)
set(VCPKG_LIBRARY_LINKAGE static)
set(VCPKG_CMAKE_SYSTEM_NAME Linux)
set(VCPKG_BUILD_TYPE release)
set(VCPKG_C_FLAGS "-fPIC -fvisibility=hidden")
set(VCPKG_CXX_FLAGS "-fPIC -fvisibility=hidden -fvisibility-inlines-hidden")
set(VCPKG_C_FLAGS_RELEASE "-O2")
set(VCPKG_CXX_FLAGS_RELEASE "-O2")
