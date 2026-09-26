# For agents working on VidCrop

- Read `specs/README.md` first; `specs/api.md` explains how to drive and test everything.
- Every feature must be reachable through the core's JSON commands (`crates/core/src/session.rs`), not only through the page. The page and the HTTP API are both thin front ends to it.
- Test by driving the API (`cargo test`, `vidcrop serve`, `vidcrop api '{...}'`); use DesktopIA or screenshots only to check looks and mouse gestures.
- Run the app: `npx tauri dev`. The running app writes its API address and token to `~/Library/Application Support/VidCrop/api.json` (macOS).
- Only the first video and first audio stream are saved; saving always writes a new file and never touches the original.
