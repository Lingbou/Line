# Line

Line is a lightweight terminal SSH connection manager. It stores connection
profiles locally and hands the actual connection to the system OpenSSH client.

## Language

**Profile**:
A saved connection identity containing a name, endpoint, authentication
settings, and optional connection behavior.
_Avoid_: Server, host entry, config entry

**Direct Connection**:
A connection whose jump chain is absent or empty.
_Avoid_: Normal mode, plain SSH

**Jump Hop**:
One intermediate SSH endpoint used before reaching the target.
_Avoid_: Proxy, gateway, relay

**Jump Chain**:
An ordered list of jump hops. Hop order is part of the connection meaning.
_Avoid_: Proxy list, jump list

**Platform Target**:
An operating system and CPU architecture supported by a release artifact.
_Avoid_: Build, port

**Distribution Target**:
A Linux distribution and package format supported by a release artifact.
_Avoid_: Package, installer

**Release Artifact**:
A downloadable package, archive, or single-file binary produced for a platform target.
_Avoid_: Build output, bundle

**OpenSSH Baseline**:
The minimum OpenSSH capability required by a connection mode.
_Avoid_: SSH version
