# Paisley rename

The user approved Paisley as the browser name, with command `paisley` and
proposed Cargo package `vox-paisley`. Update application labels, Rust crate
references, MCP names, packaging and current documentation together. Keep the
repository checkout and remote as Browny. Keep profile directories, lock names
and serialized session sentinels compatible with existing saved state. No
profile migration is needed. Historical design and evidence documents retain
the names that were used at the time. Install and show the renamed application
in Bot Screen. Publishing a Cargo crate is outside this rename.

Approach: rename the product and executable in place, preserving its storage
format. A full storage migration would add failure cases without improving the
visible rename; changing only the display name would leave CLI and packaging
inconsistent.
