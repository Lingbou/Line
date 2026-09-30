# ProxyJump representation

**Status:** accepted

A connection profile may contain an ordered `jump_chain`. An absent or empty chain means a direct connection.

Each hop has one of two forms:

- an endpoint containing user, host, and optional port
- a reference to another saved profile

The chain is persisted in order and rendered in that order. A pasted `ssh -J` command produces endpoint hops; a user-selected saved connection produces a profile reference.

Command construction resolves the chain to OpenSSH's `-J` representation. The order is preserved exactly. Line does not silently fall back to a direct connection if a hop fails; the error identifies the failing hop when possible.

Deleting a profile does not cascade into profiles that reference it. A stale reference is reported as an actionable error until the user repairs or removes it.

Line supports raw endpoint hops. For a key-authenticated profile, Line builds an OpenSSH `ProxyCommand` using the same profile identity for every hop and for the target connection. OpenSSH does not apply the target `-i`/`IdentitiesOnly` options to the inner connection created by `-J`, so this preserves the behavior users expect from a pasted `ssh -J` command while keeping Line's profile authentication deterministic.

Multi-hop chains nest that command once per hop. The first hop is connected directly, and each later hop is reached by nesting the previous command as its `ProxyCommand`, with the last hop as the destination of the command that carries `-W %h:%p` for the target:

```text
ssh -o ProxyCommand="ssh ... -W %h:%p -- first-hop" -W %h:%p -- second-hop
```

OpenSSH expands `%h:%p` against the destination of the process whose configuration carries the ProxyCommand, so every level forwards to the right address without Line resolving the intermediate addresses itself.

Because each hop runs its own `ssh`, a failure names the hop it could not reach. Line reads the OpenSSH diagnostics for whole host tokens, reports the first hop they single out as `jump i/n`, and leaves the raw diagnostics intact.

Profile-backed hops and TUI rendering follow in dependent tickets.
