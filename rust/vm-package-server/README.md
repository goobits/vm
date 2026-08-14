# vm-package-server

Private registry data plane for VM's managed package appliance. It serves npm,
Cargo, Python, and tool artifacts in the central appliance, and runs as the
read-only cache/proxy edge attached to each managed environment.

Operators should not run this component directly or publish with npm, Cargo,
pip, or Twine. VM owns authentication, client configuration, source review,
immutable publication, and service lifecycle:

```bash
vm packages up
vm packages status
```

Agents change registered packages and tool collections through an isolated
guest checkout:

```bash
vm packages checkout <source> --agent <agent> --task "<task>"
# Edit the returned checkout, bump its version, and commit.
vm packages release <checkout-id>
```

The release worker publishes only to the private VM gateway. See the canonical
[Package Infrastructure guide](../../docs/user-guide/package-infrastructure.md)
for setup, release, consumer-update, security, and recovery behavior.

## Development

The standalone binary exists for appliance images and integration tests. Its
only command starts one server process:

```bash
cargo run -p vm-package-server --features standalone-binary -- \
  start --host 127.0.0.1 --port 3080 --data ./data
```

Run the crate checks from the Rust workspace:

```bash
cargo test -p vm-package-server
cargo clippy -p vm-package-server --all-targets --all-features -- -D warnings
```

HTTP routing and authentication are implementation APIs owned by `src/server.rs`
and `src/auth.rs`; they are not a separate public operator surface.
