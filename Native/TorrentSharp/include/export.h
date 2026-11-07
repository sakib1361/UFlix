#pragma once

#if defined(_WIN32)
#  if defined(TSDL_BUILDING)
#    define TSDL_API __declspec(dllexport)
#  else
#    define TSDL_API __declspec(dllimport)
#  endif
#else
#  define TSDL_API __attribute__((visibility("default")))
#endif
