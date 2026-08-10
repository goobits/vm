# vm-packages

Shared, provider-neutral package-infrastructure types and the read-only gateway
client used by VM Tool.

This crate deliberately does not install packages, run builds, access Git, or
mount host package directories. Privileged and mutable work belongs to the
managed package-infrastructure services.

The canonical operator and architecture guide is
[`docs/user-guide/package-infrastructure.md`](../../docs/user-guide/package-infrastructure.md).
