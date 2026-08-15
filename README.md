# shesh-kernel

> **Archived. Superseded by [shesh-aos](https://github.com/gaganjainse/shesh-aos).**
>
> Per [ADR-0008](https://github.com/gaganjainse/shesh-docs/blob/main/src/governance/adr/0008-kernel-archive.md)
> the two Rust trees were not force-merged, because doing so would have shipped a
> broken build. `shesh-aos` is the source of truth for the Rust workspace. This
> repository is kept because it still holds work that has not been rebased across:
> the `gui`, `protocols`, `terminal`, `tui`, and `worker` crates exist only here.
>
> The crates below keep their original pre-rename names on purpose. Renaming them
> would produce crates identically named to the ones in `shesh-aos` while the two
> trees have genuinely diverged, which is the collision recorded as F020 in the
> failure register. Names change when the staged rebase in ADR-0008 lands, not
> before.

## Status

| | |
|---|---|
| **State** | Archived, not maintained |
| **Superseded by** | [shesh-aos](https://github.com/gaganjainse/shesh-aos) |
| **Licence** | MIT (see LICENSE) |
| **Merge plan** | ADR-0008, staged rebase, leaf crates first |

Do not start new work here. Open issues against `shesh-aos`.

## What is still only here

| Crate | Why it has not moved |
|---|---|
| `nexusaos-gui` | No counterpart in shesh-aos |
| `nexusaos-protocols` | ACP and MCP wire implementations, step 4 of the rebase |
| `nexusaos-terminal` | No counterpart in shesh-aos |
| `nexusaos-tui` | API diverged from the shesh-aos TUI; needs reconciliation |
| `nexusaos-worker` | Isolated tool-execution binary |

## Building

```bash
git clone https://github.com/gaganjainse/shesh-kernel.git
cd shesh-kernel
cargo build --release
cargo test --workspace
```

## Historical description

The original project description is preserved verbatim at
[docs/archive/original-readme.md](docs/archive/original-readme.md). It records
the system as it was before the archive decision, keeps the pre-rename naming
that matches the crates on disk, and is not maintained.
