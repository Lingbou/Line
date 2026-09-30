# ProxyJump representation

**Status:** accepted

A connection profile may contain an ordered `jump_chain`. An absent or empty chain means a direct connection.

Each hop has one of two forms:

- an endpoint containing user, host, and optional port
- a reference to another saved profile

The chain is persisted in order and rendered in that order. A pasted `ssh -J` command produces endpoint hops; a user-selected saved connection produces a profile reference.

Command construction resolves the chain to OpenSSH's `-J` representation. The order is preserved exactly. Line does not silently fall back to a direct connection if a hop fails; the error identifies the failing hop when possible.

Deleting a profile does not cascade into profiles that reference it. A stale reference is reported as an actionable error until the user repairs or removes it.

The first implementation slice supports raw endpoint hops. For a single key-authenticated hop, Line builds an OpenSSH `ProxyCommand` using the same profile identity for both the jump and target connections. OpenSSH does not apply the target `-i`/`IdentitiesOnly` options to the inner connection created by `-J`, so this preserves the behavior users expect from a pasted `ssh -J` command while keeping Line's profile authentication deterministic.

Profile-backed hops, multi-hop chains, and TUI rendering follow in dependent tickets.
