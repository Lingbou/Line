# Shared assertions for the container install smoke tests.
#
# Static linkage is asserted where the artifact is built, not here: glibc's
# ldd reports static binaries correctly, but busybox ldd on Alpine reports the
# musl loader path even for a static-pie binary.
#
# Sourced by the install-*.sh scripts, which run inside distribution
# containers. Keep this file POSIX: Debian images provide dash, Alpine
# provides busybox ash, and the RPM family provides bash.

smoke_fail() {
  printf 'smoke: %s\n' "$*" >&2
  exit 1
}

# The installed command must start and answer both non-interactive modes.
smoke_assert_runs() {
  LINE_CONFIG_DIR="${LINE_CONFIG_DIR:-/tmp/line-smoke-config}"
  export LINE_CONFIG_DIR

  version="$(line --version)" || smoke_fail "line --version failed"
  printf '%s\n' "$version" | grep -Eq '^line [0-9]+\.[0-9]+\.[0-9]+' ||
    smoke_fail "unexpected version output: $version"

  line --list > /dev/null || smoke_fail "line --list failed"
}

# The installed command must be able to reach the OpenSSH programs Line drives.
smoke_assert_ssh_keygen() {
  command -v ssh-keygen > /dev/null || smoke_fail "ssh-keygen is missing"
}
