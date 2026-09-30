#!/bin/sh
# Unpack the single-file tarball inside an Arch or Alpine container and prove
# it runs against that distribution's own OpenSSH programs.
#
# Usage: install-tarball.sh <path to .tar.gz inside the container>
set -eu

smoke_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
. "$smoke_dir/lib.sh"

tarball="${1:?usage: install-tarball.sh <line-<version>-<target>.tar.gz>}"

if command -v apk > /dev/null 2>&1; then
  apk add --no-cache openssh-client openssh-keygen > /dev/null
elif command -v pacman > /dev/null 2>&1; then
  pacman -Sy --noconfirm --needed openssh > /dev/null
else
  smoke_fail "no supported package manager in this image"
fi
smoke_assert_ssh_keygen

mkdir -p /opt/line
tar -xzf "$tarball" -C /opt/line --strip-components=1
ln -sf /opt/line/line /usr/local/bin/line

smoke_assert_runs
