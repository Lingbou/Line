# Platform support matrix

**Status:** accepted

Line v0.2 targets the following platform matrix:

- Linux x86_64 and aarch64, with `.deb`, `.rpm`, and static musl single-file releases.
- Arch Linux and Alpine Linux through the static musl release, installed next to the distribution's own OpenSSH programs.
- macOS on both Intel and Apple Silicon, shipped as a single universal2 tarball.
- Native Windows x86_64 and arm64 binaries, using the system OpenSSH client in Windows Terminal.
- WSL uses the Linux artifacts: `.deb`, `.rpm`, or the static tarball depending on the distribution.

OpenSSH 7.6 is the baseline for key-based connections. Saved-password connections retain the OpenSSH 8.4 requirement because they use `SSH_ASKPASS_REQUIRE=force`.

The single-file distributions are first-class artifacts, not fallbacks: Linux static musl, macOS universal2, and Windows native archives.

## Distribution install paths (v0.2)

Each distribution target has one documented install path. The OpenSSH package
names differ per distribution, and Line depends on both the client and
`ssh-keygen`, which not every distribution ships in the package that carries
the `ssh` executable:

- Debian and Ubuntu ship both programs in `openssh-client`.
- The RPM family ships `ssh-keygen` in `openssh`, which `openssh-clients` depends on.
- Alpine ships `ssh-keygen` in `openssh-keygen`, which `openssh-client` pulls in.
- Arch ships both programs in `openssh`.

| Target | Install path | OpenSSH dependency |
| --- | --- | --- |
| Debian, Ubuntu | `.deb` from the release page | `openssh-client` provides `ssh` and `ssh-keygen` |
| Fedora, RHEL, Rocky, Alma | `.rpm` from the release page | `openssh-clients` pulls in `openssh`, which is where `ssh-keygen` lives |
| Arch Linux | static tarball | `openssh` provides `ssh` and `ssh-keygen` |
| Alpine Linux | static tarball | `openssh-client` pulls in `openssh-keygen`, which is a separate package on Alpine |

The `.deb` and `.rpm` install paths are smoke-tested in CI by installing the
package inside the distribution's own container image. The Arch and Alpine
static paths are smoke-tested by unpacking the release tarball inside
`archlinux` and `alpine` containers that have the distribution OpenSSH packages
installed.

Updating on Arch and Alpine means unpacking the newer release tarball over
the installed binary; there is no package database entry to upgrade in place.

macOS uses the system OpenSSH that ships with the OS, so the release page is
the only install path in v0.2: users unpack the universal2 tarball and put
`line` on their `PATH`. A Homebrew formula is a follow-up for the same reason
as the AUR entry below — it needs a tap that a maintainer owns and that can be
smoke-tested automatically, rather than a build step CI can drive on its own.

The macOS artifact is built by compiling both Apple targets on a macOS runner
and merging them with `lipo`, so one download runs on Intel and Apple Silicon.

## Windows

Windows runs the same binary shape as the other platforms and uses the OpenSSH
client that ships as an optional Windows feature. The platform seam answers
its questions like this:

- Config root: `%USERPROFILE%\.line`, still overridable with `LINE_CONFIG_DIR`.
- OpenSSH: `ssh.exe` and `ssh-keygen.exe` from `PATH`.
- System policy: `%ProgramData%\ssh\ssh_config` when it exists, otherwise the
  `NUL` device. The user's own `%USERPROFILE%\.ssh\config` still cannot
  redefine what a saved profile means.
- File modes: Windows has no POSIX mode bits. Line does not pretend to tighten
  anything there and relies on the ACL of the user profile directory.
- Signals: Windows reports exit codes rather than signals and delivers console
  control events instead. Ctrl-C, Ctrl-Break, and console close set the
  shutdown flag; terminal restoration runs off that flag and the `Drop` guard
  rather than off a POSIX handler.
- Child cleanup: `taskkill /P <pid> /T /F` walks the process tree the way the
  Unix collectors do.

Saved passwords need `SSH_ASKPASS`, which Windows OpenSSH does not implement,
so a password profile is refused on Windows with an actionable message instead
of being launched into an interactive prompt Line cannot answer.

AUR and `.apk` packaging are follow-ups, not v0.2 deliverables. Revisit the AUR
when a maintainer owns the AUR repository and the PKGBUILD can be smoke-tested
automatically. Revisit a native `.apk` when automated builds of a signed
package are cheaper to maintain than the static tarball, which already covers
Alpine.

## Consequences

- CI must build and smoke-test more than one architecture.
- Windows and macOS packaging are separate release paths, not Linux package variants.
- WSL support is documentation and Linux-package reuse, not a separate runtime.
- ARMv7 and other architectures are not part of v0.2 unless a later ADR adds them.
