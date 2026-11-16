# Period key regression and patch release

Reproduced on the alpha.3 CEF engine with native Bot Screen keystrokes:
`a`, period, `b` gives `ab`; DOM reports Delete/keyCode46/codePeriod.
The runner incorrectly uses ASCII as a Windows virtual-key code.

1. Isolate the existing virtual-key conversion without behavior changes; add
   failing unit coverage for period and punctuation, then map to Chromium's
   actual OEM virtual-key constants while retaining separate Unicode text events.
2. Exercise real CEF text/email/textarea inputs, period versus Delete, punctuation
   and shortcut behavior. Verify native GTK input remains unaffected. Run focused
   tests and independent review. Every compilation: hard3GB, swap0, Cargo jobs1.
3. Version the changed GTK adapter and root crate. Through Bot Screen as Vox,
   publish the adapter, verify the root package against its published dependency,
   then publish root and GitHub prerelease. Verify downloads/checksums and revoke
   temporary scoped credentials. Retain production sandbox policy; only temporary
   debug test-support native checks may use the approved root exception.

Additional user request: stabilize inspector opening and interaction. Reproduced
closing after a 2000-message console burst. Cover FIFO, 4 MiB payload bounds and
4096-entry overhead bound; verify live DevTools survives the same burst.
