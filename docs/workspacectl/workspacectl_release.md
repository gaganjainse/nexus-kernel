# workspacectl Release Plan

## 1. Purpose
This document defines how the project should be packaged, verified, and released once implementation is complete. It exists to prevent a half-finished archive from being mistaken for a final release.

## 2. Release prerequisites
A release is allowed only if:
- the workspace builds cleanly,
- the test suite passes,
- the daemon is functional,
- rollback is verified,
- the docs match behavior,
- and the package contains all mandatory files.

If any prerequisite fails, do not create a final release artifact.

## 3. Required release contents
The release archive must include:
- source code,
- `Cargo.lock`,
- `README.md`,
- `LICENSE`,
- `ATTRIBUTION.md`,
- sample config,
- systemd user service,
- CI workflow,
- installer script,
- rollback notes,
- and release notes.

If build artifacts are included, they must be clearly labeled and built for the target platform.

## 4. Versioning policy
Use semantic versioning:
- `MAJOR` for breaking changes,
- `MINOR` for new features,
- `PATCH` for fixes.

If the project is still pre-1.0, use `0.x.y` and clearly label stability expectations.

## 5. Release artifact strategy
Preferred order:
1. zipped source release,
2. optional compiled binary release,
3. checksums,
4. signed notes if signing is available.

Fallbacks:
- If zip creation fails, create a tarball temporarily and document the issue.
- If compiled binaries cannot be produced in the environment, release source plus build instructions only.

## 6. Build verification steps
Before packaging:
- run formatting checks,
- run lint checks if configured,
- run unit tests,
- run integration tests,
- run a dry-run `plan`,
- run at least one safe `organize` test on a temporary workspace,
- run `rollback` on that test,
- and verify the daemon starts and stops cleanly.

If any of these checks fail, stop the release.

## 7. Manual validation checklist
- Confirm that hidden config folders are still ignored.
- Confirm that no overwrite paths exist in planner output.
- Confirm that conflict resolution is visible.
- Confirm that rollback logs are complete.
- Confirm that `doctor` reports the current environment accurately.
- Confirm that `watch` debounces rapid file writes.
- Confirm that AI fallback is optional and disabled by default.

## 8. Packaging steps
1. Create a clean staging directory.
2. Copy only the approved files.
3. Remove temporary build outputs unless they are explicitly part of the release.
4. Include docs and examples.
5. Include service and installer files.
6. Generate the zip archive.
7. Generate a checksum file.
8. Verify the archive opens correctly.
9. Record the exact archive path in the release notes.

## 9. Release notes requirements
Release notes should list:
- version number,
- new commands,
- safety behavior,
- known limitations,
- breaking changes,
- rollback changes,
- and build environment used.

The release notes must also state any fallback behavior that is intentionally left in place.

## 10. Known limitation policy
Every release must disclose:
- any unimplemented optional features,
- any operating-system-specific caveats,
- any known watch-mode limitations,
- any AI-backend dependencies,
- any temporary packaging workarounds.

Do not hide limitations.

## 11. Rollback and recovery notes
The release must tell the user:
- where the journal is stored,
- how to undo the last action,
- how to rollback a selected set of actions,
- and how to recover if the journal is partially damaged.

If rollback is not guaranteed for a scenario, the release notes must say so.

## 12. Post-release verification
After creating the archive, verify:
- the archive contains the expected top-level files,
- the docs render correctly,
- the install script has execute permissions if intended,
- the service file path is correct,
- and the version number is consistent across files.

If any verification fails, rebuild the artifact before announcing a release.

## 13. No-fake-release rule
Do not describe a scaffold, prototype, or partial code drop as a final release.
The release is only final when:
- it builds,
- it tests,
- it packages,
- and it can be installed and used according to the docs.

