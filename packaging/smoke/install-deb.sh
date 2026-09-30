#!/bin/sh
# Install the Debian package inside a Debian-family container and prove it runs.
#
# Usage: install-deb.sh <path to .deb inside the container>
set -eu

smoke_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
. "$smoke_dir/lib.sh"

package="${1:?usage: install-deb.sh <package.deb>}"

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends "$package" > /dev/null

# The declared dependency must have pulled in ssh and ssh-keygen.
dpkg -s openssh-client > /dev/null || smoke_fail "openssh-client was not pulled in by the package"
smoke_assert_ssh_keygen

smoke_assert_runs
