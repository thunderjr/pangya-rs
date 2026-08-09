# Patches against vendored references

**Moved.** The Rugburn patch this project used to carry now lives with the installer that ships
it: <https://github.com/thunderjr/pangya-client>, under `patches/`.

That includes `rugburn-allow-multiple-instances.patch` — the `AllowMultipleInstances` option used
to run two clients on one host, which
[`docs/RUNNING_THE_CLIENT.md`](../docs/RUNNING_THE_CLIENT.md) §"Running two clients on one host"
still describes as part of the versus-hole procedure.

Nothing else lived here. If a future change to a vendored reference in `opensource-references/`
needs to survive a clean checkout, this directory is still the right place for it: that tree is
gitignored, so a diff here is the only durable record.
