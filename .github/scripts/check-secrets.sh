#!/usr/bin/env bash
# Release gate: the API keys (names from secrets.env.example) must be set as repo secrets and passed in as env vars.
# Required: TMDB (catalog) and at least one subtitle provider. The rest only warns.
set -euo pipefail

example="$(dirname "$0")/../../secrets.env.example"
mapfile -t names < <(sed -n 's/^\(UFLIX_[A-Z_]*\)=.*/\1/p' "$example")
[ ${#names[@]} -gt 0 ] || { echo "::error::no UFLIX_* names found in secrets.env.example"; exit 1; }

missing=()
for n in "${names[@]}"; do [ -n "${!n:-}" ] || missing+=("$n"); done

fail=0
if [ -z "${UFLIX_TMDB_API_KEY:-}" ]; then
    echo "::error::UFLIX_TMDB_API_KEY is not set (Settings → Secrets and variables → Actions): Discover and the catalog would not work"
    fail=1
fi
if [ -z "${UFLIX_OPENSUBTITLES_API_KEY:-}" ] && [ -z "${UFLIX_SUBDL_API_KEY:-}" ]; then
    echo "::error::neither UFLIX_OPENSUBTITLES_API_KEY nor UFLIX_SUBDL_API_KEY is set: subtitle search would not work"
    fail=1
fi
for n in "${missing[@]}"; do echo "::warning::$n is not set"; done
[ $fail -eq 0 ] && echo "API key secrets OK (${#names[@]} known, ${#missing[@]} unset)"
exit $fail
