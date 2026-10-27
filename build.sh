#!/bin/sh
# Builds the launcher in release mode and packs it into dist/.
#   sh build.sh          build and pack
#   sh build.sh --test   run the tests first
set -eu
cd "$(dirname "$0")"

if [ "${1:-}" = "--test" ]; then
    cargo test --locked --workspace
fi
cargo build --locked --release -p sjk-launcher

case "$(uname -s)" in Darwin) os=macos ;; *) os=linux ;; esac
case "$(uname -m)" in arm64 | aarch64) arch=arm64 ;; *) arch=x64 ;; esac
name="sjk-launcher-$os-$arch"
stage="dist/$name"
rm -rf "$stage" "dist/$name.tar.gz"
mkdir -p "$stage"
# The fonts are built into the binary; their licence asks to travel with them.
cp target/release/sjk-launcher assets/fonts/*-OFL.txt "$stage/"
tar -czf "dist/$name.tar.gz" -C "$stage" sjk-launcher Exo2-OFL.txt Rajdhani-OFL.txt
echo "Built dist/$name.tar.gz"
