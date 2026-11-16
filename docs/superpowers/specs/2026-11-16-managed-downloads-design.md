# Durable managed downloads

Approved intent: fast transfers, safe partial files, crash recovery, validated
resume, retries and honest completion states; no absolute corruption guarantee.

Implement an explicit direct-URL manager within Downloads. It uses Rust HTTP
workers, identity encoding, strong ETag and exact Content-Range validation for
segmented transfer/resume, synced segment checkpoints with SHA-256, and
no-overwrite atomic publication on the destination filesystem. An optional
publisher SHA-256 distinguishes checksum verification from transfer completion.
Use at most four connections per file, two running jobs, bounded buffers,
cooperative pause and per-file speed control. Restore interrupted jobs paused,
never silently restart network activity on startup. Authenticated browser
transfers remain on WebKit to preserve session/request semantics; do not replay
cookies, POST bodies, credentials or expiring browser grants into another client.
Managed jobs are unavailable in private windows to avoid persistent URL-bearing recovery metadata. Files explicitly saved
by private downloads remain on disk. Native browser transfers also stage partial
files and publish only on checked completion.

No automated tests are added or run unless the user requests testing. Build,
source review and direct UI inspection are the evidence available this session.
