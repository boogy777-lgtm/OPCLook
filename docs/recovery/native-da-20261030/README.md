# Native DA recovery checkpoint — 2026-10-30

This is a recovery artifact, not a completed migration or a buildable replacement backend.
The managed execution environment became unavailable while publishing the tested local
Gateway/scalar work. The files under `files/` are exact Git blobs uploaded before
the interruption. `manifest.json` records their source identities and the missing
files, local worktrees, measured checks and open work. These copies are not used
by the application build.

The active application remains in draft PR #18. Its published I4 SDK bridge
and independent local COM server are separate from this partial snapshot.
The final application head and CI should be read from PR #18, not inferred from
this artifact. No local test or historical CI proves an unexecuted Windows path.

Resume from the actual application head, restore the managed environment and
inspect both local worktrees before discarding anything. Restore files only with
matching Git blob IDs; do not infer missing code from a status sentence. The
client worktree contains uncommitted Unicode/BSTR work that is not in this
artifact. Re-run the actual Gateway live tests and full exact-head CI before
production switching; remove the legacy data plane only after the native path
covers its required capabilities.
