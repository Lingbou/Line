# Platform support matrix

**Status:** accepted

Line v0.2 targets the following platform matrix:

- Linux x86_64 and aarch64, with `.deb`, `.rpm`, and static musl single-file releases.
- Arch Linux through an AUR/source package, with the static binary as a fallback.
- Alpine Linux through the static musl release, with `.apk` packaging as a follow-up if the maintenance cost stays reasonable.
- macOS on both Intel and Apple Silicon, shipped as a universal2 artifact and installable through Homebrew.
- Native Windows x86_64 and arm64 binaries, using the system OpenSSH client in Windows Terminal.
- WSL uses the Linux artifacts: `.deb`, `.rpm`, or the Arch/source package depending on the distribution.

OpenSSH 7.6 is the baseline for key-based connections. Saved-password connections retain the OpenSSH 8.4 requirement because they use `SSH_ASKPASS_REQUIRE=force`.

The single-file distributions are first-class artifacts, not fallbacks: Linux static musl, macOS universal2, and Windows native archives.

## Consequences

- CI must build and smoke-test more than one architecture.
- Windows and macOS packaging are separate release paths, not Linux package variants.
- WSL support is documentation and Linux-package reuse, not a separate runtime.
- ARMv7 and other architectures are not part of v0.2 unless a later ADR adds them.
