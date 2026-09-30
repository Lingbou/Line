#!/usr/bin/env bash
# Build the single-file release tarball for one target from artifacts that were
# already compiled into the shared Cargo target directory.
#
# Usage: packaging/make-tarball.sh <version> <target-triple> <output-dir>
#
# The tarball contains the static binary, LICENSE, and README.md, unpacked
# into a versioned directory:
#
#   line-v<version>-<target-triple>.tar.gz
#   └── line-v<version>-<target-triple>/
set -euo pipefail

version="${1:?usage: make-tarball.sh <version> <target-triple> <output-dir>}"
target="${2:?usage: make-tarball.sh <version> <target-triple> <output-dir>}"
out_dir="${3:?usage: make-tarball.sh <version> <target-triple> <output-dir>}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$root/target/$target/release/line"
package="line-v$version-$target"
stage="$out_dir/$package"

if [ ! -f "$binary" ]; then
  echo "make-tarball: missing release binary: $binary" >&2
  echo "make-tarball: build it first with: cargo build --release --locked --target $target" >&2
  exit 1
fi

mkdir -p "$stage"
cp "$binary" "$stage/"
cp "$root/README.md" "$root/LICENSE" "$stage/"
tar -czf "$out_dir/$package.tar.gz" -C "$out_dir" "$package"
rm -rf "$stage"

echo "$out_dir/$package.tar.gz"
