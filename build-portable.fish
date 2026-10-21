#!/usr/bin/env fish

argparse d/debug n/no-clean h/help -- $argv
or exit 1

if set -q _flag_help
    echo "Usage: build-portable.fish [--debug] [--no-clean]"
    exit 0
end

set -l bin_name eventsfx
set -l interpreter /lib64/ld-linux-x86-64.so.2

for tool in cargo patchelf
    if not command -q $tool
        echo "error: '$tool' not found" >&2
        exit 1
    end
end

if set -q _flag_debug
    set -f profile_dir debug
    set -f build_args
else
    set -f profile_dir release
    set -f build_args --release
end

if not set -q _flag_no_clean
    echo "===> cleaning build artifacts"
    cargo clean; or exit 1
end

echo "===> building ($profile_dir)"
cargo build $build_args; or exit 1

set -l target_dir (cargo metadata --format-version 1 --no-deps 2>/dev/null \
    | string match -rg '"target_directory":"([^"]+)"')
test -n "$target_dir"; or set target_dir target

set -l bin "$target_dir/$profile_dir/$bin_name"
if not test -f "$bin"
    echo "error: binary not found at $bin" >&2
    exit 1
end

set -l portable "$bin-portable"
cp "$bin" "$portable"; or exit 1

echo "==> Patching $portable"
patchelf --set-interpreter $interpreter --remove-rpath "$portable"; or exit 1

echo "==> Done"
echo "    interpreter: "(patchelf --print-interpreter "$portable")
echo "    rpath:       "(patchelf --print-rpath "$portable"; or echo "(none)")
echo "    output:      $portable"
