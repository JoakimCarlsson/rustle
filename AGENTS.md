# Rustle

Rustle is a standalone declarative UI framework: a Cargo workspace of `rustle-*` crates, published on its own. Read `docs/architecture.md` first: it lists what each crate holds, what does not belong in it, and where new things go.

## Rules

- Rustle never depends on Pyre. No crate, manifest, comment, doc or example here names, depends on or mentions Pyre or any other engine or product that uses it. Support for a host that embeds Rustle is described in Rustle's own terms: a host that owns the window, the GPU device and the frame (`rustle::Embedded`, `rustle_wgpu::Renderer::with_device`). It may name what Rustle builds on, such as Compose and Phosphor.
- Dependencies between crates point down the layers in `docs/architecture.md`. A renderer depends on nothing above `rustle-graphics`; the runtime never sees a `Node`.
- Committed manifests name versions or git sources, never absolute or sibling-folder paths. Change dependencies with `cargo add` and `cargo remove`, not by hand.
- Match the style of the surrounding code: its naming, its comment density, a doc comment on every public item written for the person using it, and no new idiom where the file already has one.
- `make fmt` and `make lint` (`cargo clippy --workspace --all-targets -- -D warnings`) before finishing. A few clippy errors in `examples/advance/field.rs` and the `rustle-ui` layout tests were there before and are not part of other changes.
- Keep `docs/architecture.md` and `README.md` true to what changed: the crate table, the frame flow and the limitations.

## Commands

- `make run`: the `hello_world` showcase.
- `cargo run --example embedded -p rustle`: checks the embedding API headless (needs a GPU adapter).
- `cargo test --workspace`.
- `RUSTLE_SNAPSHOT=out.png` draws one frame without a window (see `docs/development.md`).

## Layout of `rustle`

The `rustle` crate holds the public API and the platform side. `interface.rs` is the window-free core of input handling that `App` (native windows) and `Embedded` (a host's area) share: nothing in it may touch a window. Window effects are recorded there and applied by `App`.

Build output stays in `target/`; do not write to `/tmp`. Commits are the maintainer's.
