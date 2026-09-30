#!/bin/sh
# Install the RPM package inside an RPM-family container and prove it runs.
#
# Usage: install-rpm.sh <path to .rpm inside the container>
set -eu

smoke_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
. "$smoke_dir/lib.sh"

package="${1:?usage: install-rpm.sh <package.rpm>}"

dnf install -y "$package"

# The declared dependency must have pulled in ssh and ssh-keygen.
rpm -q openssh-clients > /dev/null || smoke_fail "openssh-clients was not pulled in by the package"
smoke_assert_ssh_keygen

# The installed metadata must not ask for a glibc symbol version.
requires="$(rpm -q --requires line)"
printf '%s\n' "$requires" | grep -q 'GLIBC_' &&
  smoke_fail "package requires a GLIBC symbol version: $requires"

smoke_assert_runs
