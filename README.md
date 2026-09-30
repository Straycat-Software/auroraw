# Auroraw

Free and open source application for photographers: organise, develop (RAW files and film
negative scans, non-destructively), convert, edit metadata and deliver galleries.

> **Status: milestone M1 started.** The design is complete ([documents below](#documents)) and the
> repository has its product layout (work package WP0). There is nothing to install yet.

## Documents

- **[User manual](docs/manual/README.md)**: how to use what the application does today
- [Functional specification](docs/functional-specification.md)
- [Decision log](docs/decisions.md)
- [Architecture](docs/architecture.md)
- [Testing strategy](docs/testing-strategy.md)
- [Continuous integration and releases](docs/continuous-integration.md)
- [Governance and contributions](docs/governance.md)
- [Milestone M1 plan](docs/m1-plan.md), [milestone M2 plan](docs/m2-plan.md) and the [interface port's parity checklist](docs/ui-parity-checklist.md)
- [Technical spikes](docs/technical-spikes.md)
- Design notes: [001 workspace layout](docs/design/001-workspace-layout.md), [002 state files](docs/design/002-state-files.md), [003 sidecars](docs/design/003-sidecars.md), [004 fingerprint](docs/design/004-fingerprint.md), [005 image engine interfaces](docs/design/005-image-engine-interfaces.md), [006 place names](docs/design/006-place-names.md)

## Building

The toolchain is pinned in `rust-toolchain.toml`; `rustup` installs it on first use. The interface is Qt
Quick (D-094), so building also needs **Qt 6.4 or later**, **cmake 3.24 or later**, a C++ compiler and
`qmake` on the path (or in `QMAKE`); the packages per platform are in [CONTRIBUTING.md](CONTRIBUTING.md).

```bash
cargo build --workspace
cargo test --workspace
cargo xtask check      # licence headers and the allowed dependencies between crates
cargo run -p auroraw-app                 # the application
```

The crates are described in the [architecture](docs/architecture.md) (§3). The code of the four
technical spikes is archived at the tag `spikes-final`.

## License

GPL-3.0-or-later, see [LICENSE](LICENSE). The plugin API and SDK will be MIT OR Apache-2.0, and plugins that use only that API may have any licence ([PLUGIN-EXCEPTION](PLUGIN-EXCEPTION)). The documentation is CC BY-SA 4.0 ([docs/LICENSE](docs/LICENSE)). To contribute, see [CONTRIBUTING.md](CONTRIBUTING.md).
