# CEF default and WebKit deprecation

User decision: focus new development on CEF; deprecate WebKit.

1. Add regression checks for default feature selection, a runtime-aware launcher,
   desktop launch identity, and packaging of both executables.
2. Make CEF default. Keep WebKit behind explicit `--no-default-features
   --features legacy-webkit` for migration only; no silent fallback.
3. Split the small `paisley` launcher from `paisley-engine`. Locate a complete
   installed CEF runtime before executing the engine; preserve arguments,
   environment, exit status, profile routing and sandbox policy. Keep desktop
   entries pointing to the launcher. Do not download code on browser startup.
4. Update install instructions, package tooling and backend support status.
   The published alpha.2 remains unchanged until a new release is requested.
5. Run focused regression coverage and both backend checks under a hard decimal
   3 GB cap, zero build swap and one Cargo job. Inspect native Bot Screen behavior
   where permitted, then request a whole-change review.

CEF runtime installation and secure non-root sandbox setup remain explicit
requirements. Deprecation does not claim CEF notification, IME, HiDPI or popup
parity. WebKit SDK references may remain during migration.
