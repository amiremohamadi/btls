#!/usr/bin/env bash
set -e

cargo build
nvim --cmd ":luafile ./lua/btls.lua" test.bt
