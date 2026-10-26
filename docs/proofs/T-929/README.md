**Referenz:** Proof-Notiz zu T-929; dokumentiert Ursache, Fix und Prüfstatus, keine allgemeine Betriebsanleitung.

# T-929: Linux lease test root cause

## Root cause

The stale-lease predicate requires both an expired heartbeat and a dead owner PID. On non-Windows targets, `pid_is_alive` previously returned `true` unconditionally. Therefore a lease with a reaped worker PID could never become `Stale` on Linux; the test failure was deterministic, not a timing-only CI flake.

## Fix

- Linux checks whether `/proc/<pid>` exists.
- Other Unix targets use `kill -0 <pid>` without adding a libc dependency.
- Windows retains its existing native process-status check.
- The reclaim regression test explicitly asserts that its reaped child PID is recognized as dead.

## Verification

Pending CI for branch `fix/T-929-linux-lease-test-isolation`. Required command: `cargo test --lib config::profiles`; also check `git diff --check`.

## CI hint

The failing test is `config::profiles::lease_tests::verwaiste_lease_abgestuerzten_workers_wird_reclaimed`. Its assertion expected `SwarmProfileLeaseState::Stale`, but the unconditional non-Windows PID-alive result made `owner_is_reclaimable` false.
