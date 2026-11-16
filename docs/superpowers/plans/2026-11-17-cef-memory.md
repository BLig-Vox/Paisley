# CEF and memory residency implementation plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan in the current session.

**Goal:** Replace the abandoned Servo migration with CEF while controlling inactive-tab residency and measuring memory honestly.

**Architecture:** Keep the GTK browser shell and bounded process transport. Select CEF as an alternative native runner; share one profile engine/context across tabs. Add an engine-independent residency policy and explicit native close acknowledgments.

**Tech Stack:** Rust, GTK4, cef-rs 156.6.0, CEF, Python/systemd build supervisor.

**Spec:** ../specs/2026-11-17-cef-memory-design.md

## Global constraints

All builds: kernel 2 GiB limit, no build swap, one job, stop below 2 GiB system available memory. All desktop/account/install work through Bot Screen. No global process kills. Preserve installed browser until native validation passes.

## Review focus

- A closing page must not remain in callbacks or renderer maps.
- Unknown page activity must protect forms/media from automatic discard.
- Profile cookies must share within a profile and remain isolated across profiles/private windows.
- Frame/event queues and page snapshots must stay bounded.
- Packaged runtime, sandbox, and ABI must match Ubuntu and the installed binary.

## Tasks

- [x] Enforce the build cap: verify cgroup memory.max, memory.swap.max and a real build; write resource evidence.
- [x] Add CEF runner feature without linking Servo, preserving upstream notices; build within the cap. Validate startup and a native HTML fixture.
- [x] Map navigation, painting, input, loading/history, popup and close callbacks to the GTK transport; test native tab sharing/cleanup.
- [x] Add pure residency state/policy tests for LRU, protection, age and pressure. Bridge trustworthy page activity, discard/reload and memory monitoring.
- [ ] Restore downloads, site data/settings, permissions, notifications, inspector and favicon behavior with CEF. Verify native and regression tests.
- [ ] Measure repeated tab cycles and YouTube on Bot Screen. Resolve failures and package the matching runtime before changing the default feature/install.
- [ ] Obtain a fresh whole-change review, address findings, and report exact evidence and remaining limits.
