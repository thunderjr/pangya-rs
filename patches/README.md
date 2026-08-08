# Patches against vendored references

`opensource-references/` is gitignored, so changes made inside it are not tracked. Anything this
project needs from a modified reference lives here as a diff instead, so it survives a clean
checkout of those references.

## `rugburn-allow-multiple-instances.patch`

Applies to `pangbox/rugburn` at commit `7158511`.

Adds an `AllowMultipleInstances` option to `rugburn.json`. When set, `CreateMutexA` and
`OpenMutexA` are hooked to append a per-process suffix to every *named* mutex, so each client
instance gets its own namespace and the client's single-instance check never finds an existing one.
Unnamed mutexes are already per-process and pass through untouched.

### Why it exists

A versus room needs a second player: the client refuses to start a room holding fewer players than
its capacity, and the smallest versus capacity its Make Room dialog offers is two. It also refuses
to run twice. `ProjectG.exe` is WinLicense-packed, so the check cannot be patched statically —
hence doing it at runtime from Rugburn, which already hooks `kernel32`.

The option is off by default. It is only appropriate against a local server.

### Applying and building

```bash
cd opensource-references/pangbox--rugburn
git apply ../../patches/rugburn-allow-multiple-instances.patch
docker run --rm -v "$PWD":/w -w /w debian:bookworm-slim \
  sh -c "apt-get update -qq && apt-get install -y -qq g++-mingw-w64-i686 make && make"
# out/ijl15.dll
```

Deploy the result **only** to the secondary client install, leaving the primary one on its
unmodified `ijl15.dll`. Keep an `ijl15.dll.orig` backup of whatever it replaces.
`docs/RUNNING_THE_CLIENT.md` covers the surrounding two-instance procedure.
