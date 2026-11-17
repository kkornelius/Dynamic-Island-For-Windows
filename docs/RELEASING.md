# Release process

1. Update Cargo version/changelog and README asset URLs.
2. Fetch/build locked dependencies, regenerate notices, and run formatting, Clippy, unit tests and release build.
3. Run native/render checks with an isolated profile. Record missing media/device preconditions.
4. Generate media only from the fictional-data renderer. Review all screenshots and decoded GIF frames.
5. Run `scripts/package.ps1` and inspect its portable PE report.
6. Stage an explicit allowlist; run `python scripts/audit-public.py --staged` and review the staged diff.
7. Push reviewed source and wait for the Windows workflow. Create a matching GitHub pre-release with the EXE, ZIP and checksum file. Verify downloaded assets against local hashes.

The GitHub-generated source archives do not contain the compiled app. Binaries belong in Releases, not source history.

Do not publish working profiles, raw captures, Notes, caches, credentials or signing keys. Pattern scanning supplements manual review and cannot detect every possible secret. [Public-file policy](PUBLIC-FILES.md).

This beta is unsigned and has no installer/updater. If signing is introduced, sign before creating hashes/ZIPs and keep credentials outside Git. Preserve third-party licenses/provider credits and test data migration/removal before adding an installer.
