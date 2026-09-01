#!/usr/bin/env bash
set -euo pipefail

if (($# != 2)); then
  printf 'Usage: %s VERSION CHANGELOG\n' "$0" >&2
  exit 2
fi

version=$1
changelog=$2

awk -v version="$version" '
  $0 ~ "^## \\[" version "\\]( |$)" {
    found = 1
    print "## Dirgo " version
    next
  }
  found && /^## \[/ { exit }
  found && /^[[:space:]]*$/ { blanks++; next }
  found {
    while (blanks > 0) { print ""; blanks-- }
    print
  }
  END {
    if (!found) {
      print "Missing CHANGELOG section for " version > "/dev/stderr"
      exit 1
    }
  }
' "$changelog"
