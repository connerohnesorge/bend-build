# bend-build

`bend-build` compiles [Bend](https://github.com/bendlang/bend) from a Cargo
build script and gives Rust typed functions for selected pure definitions.
The shape is the same idea as `prost-build`: `build.rs` runs the other
compiler, the crate links the result, and Rust `include!`s generated
wrappers.

Bend is checked as part of that build. A `law` that is open or false fails
`cargo build`.

## Requirements

- Rust, stable, 2021 edition.
- The `bend` CLI on `PATH` (or `$BEND`). Install:

  ```bash
  curl -fsSL https://bend-lang.com/install.sh | sh
  export PATH="$HOME/.bend/bin:$PATH"
  ```

  This crate was developed against Bend 2.0.39. The C it post-processes is
  that compiler's private output, not a stable ABI. A newer `bend` that
  changes `main` or task layout will fail the build with an explicit error
  rather than link a wrong call.
- clang 14 or newer (`clang` on `PATH`, or `$BEND_CC`, or a `$CC` that is
  clang). Bend's C uses `#embed` and clang pragmas; gcc is not a target.
  GPU `!` calls want clang 19 in Bend itself. This crate does not enable
  the GPU. See [Limitations](#limitations).

`BEND_NO_TELEMETRY=1` is set for the `bend` process when it is not already
set, so a build does not phone home.

## Usage

```rust
// build.rs
fn main() -> Result<(), bend_build::Error> {
    bend_build::Builder::new()
        .file("src/math.bend")
        .export("add")
        .export("sum_to")
        .compile()?;
    Ok(())
}
```

`compile(&["src/math.bend"])` exports every top-level def whose parameters
and result are monomorphic `U32`, `F32`, or `Nat`, except `main`.
`.export("name")` switches that file to an explicit list. Anything else
(proofs, lists, erased arguments, `IO`) is refused with the reason.

```rust
// src/main.rs
include!(concat!(env!("OUT_DIR"), "/bend_build/math.rs"));

fn main() {
    assert_eq!(math::add(20, 22), 42);
}
```

The generated module is named after the file stem (`math.bend` → `math`,
hyphens become underscores).

| Bend | Rust |
| --- | --- |
| `U32` | `u32` |
| `F32` | `f32` |
| `Nat` | `u64` |

A `Nat` argument or result above `2^48 - 1` panics in the wrapper. Bend's
runtime treats a larger `Nat` as fatal, and the wrapper stops before that.
`U32` arithmetic wraps. `F32` is Bend's axiomatic float: the checker will
not prove facts about it, and the bits Rust passes are the bits Bend adds.

## Proofs

`bend file.bend --check-only` runs before code generation. The build
continues only when Bend prints `ALL PROOFS CHECK`. The three programs
under `examples/proven/bend/` are the ones this repo actually runs:

- `arith.bend` proves `add` is commutative and `mul(0, b) = 0`.
- `fold.bend` proves a left fold and a right fold of a `Nat` list agree,
  and Rust calls `sum_to`.
- `par.bend` proves a fork/join range sum equals the sequential sum, for
  every depth. Rust checks concrete depths against both defs.

```bash
cargo run -p proven --example arith
cargo run -p proven --example fold
cargo run -p proven --example par_sum
```

`Builder::verdict(true)` also runs `bend file.bend --verdict`, which
rechecks the Bend source with the proven BendTT kernel. That needs Lean
v4.34.0, or `$BENDTT` set to a kernel Bend has already built. The examples
turn it on when `BEND_VERDICT` is set. The kernel checks the Bend file,
not the generated harness: the harness is a calling convention, and its
host defs are not proofs. `--verdict` does not prove that the C backend or
this crate's shim match the checker.

## How it works

Bend's C backend emits one program. `int main` parses `+RTS` … `-RTS`,
builds a heap (`corpus_setup` reserves an 8 GiB virtual span), and evaluates
the `main` def. There is no header, no exported def, and no promise that a
pure def survives as its own symbol. Numeric defs are often compiled to a
file-local `spin_N`, and a flat def's segment is deleted when every call
was inlined. [Issue 813](https://github.com/bendlang/bend/issues/813)
tracks the real fix: a native library target with chosen exports, a header,
a lifecycle, and recoverable errors. It is listed under SOON in
`WONTFIX.txt` and is not scheduled. The note there still describes today's
compiler: the C output is one program with a `main`.

Until that target exists, `bend-build` does three local things:

1. It generates a harness that `import`s the Bend file and, for each
   selected def, defines `bend_build_host_<name>`. The host matches on a
   `Nat` fuel argument. The `0n` branch calls the real def. The other
   branch calls itself in non-tail position and is never taken. That
   self-call makes the host *not flat*, so Bend keeps it as a segment
   (`FID_bend_build_host_<name>`) instead of inlining it away. The
   harness `main` calls each host twice so the "called once, so inline"
   pass does not absorb the segment either.
2. It asks `bend harness.bend -o bend.c` for the C, renames `int main` so
   it cannot collide with Rust's `main`, and appends a C function in the
   same translation unit. That function is allowed to call the static
   runtime: `task_node` plus `corpus_eval`, which is how the emitted
   `main` enters a def. Arguments are immediate words (`U32` / `F32` bits
   in a `u32`, `Nat` as a `u64`). The fuel word is `0`.
3. It compiles that C with clang through the `cc` crate and writes a
   Rust module whose safe functions take a mutex, check `Nat` range, and
   call the symbol. The `extern` block is
   `#[link(name = "bend_build_<stem>", kind = "static")]` (plus `pthread`
   and `m`), so the crate that `include!`s the module is what links the
   archive. The build script also emits `cargo:rustc-link-search`.

Each Bend file is its own C program and its own heap. The static runtime
is not shared across files, which is what lets two modules link into one
binary. The heap is process-global *inside* a module. Calls are serialized.
They are not re-entrant, and a Bend fault (`err_fail`) still `_exit`s the
process. That is the same failure mode as a Bend binary. Issue 813 asks
for recoverable errors and an explicit lifecycle; this crate cannot add
either without a compiler change. When a library target ships, the harness
and the shim should be replaced by its header, and the Rust types for
`U32` / `F32` / `Nat` can stay.

Effects are a different hook, already real in Bend. An effect def imports
a `.c` twin, and the host function is named after the def (lowercased,
dots turned into underscores) and registered with `io_eff`. A Rust
`#[no_mangle] extern "C"` can implement that symbol if the `.c` file calls
it, and the `.c` is linked because Bend splices it into the program. The
names are runtime internals. Bend documents that there is no ABI promise
and that effects must be rebuilt with every compiler update. This crate
does not generate those bindings. `!` (run a pure def on the GPU) is left
on the CPU path when the emitted C can do that, and is not something this
crate configures. clang 19, Metal, or CUDA are future work.

Values that are not `U32`, `F32`, or `Nat` stay on Bend's reference-counted
heap. A `Data` type may be copied; a `Type` may not, and a safe Rust
binding would have to be non-`Clone` and `Drop` exactly when Bend drops
the last reference. The task node returns one word through `corpus_eval`
(`rv[0]`). Multi-word layouts (a two-constructor datatype, a pair) do not
fit that return, and the heap layout is private. Those bindings are not
generated. Lists in `fold.bend` are there so the checker can see the law;
Rust calls `sum_to`, which returns a `Nat`.

`cargo:rerun-if-changed` is emitted for each Bend file, the Bend files it
imports, and the `bend` binary. `$BEND`, `$BEND_CC`, and `$BENDTT` are
`rerun-if-env-changed`.

## Examples

`examples/proven` is a tiny crate whose `build.rs` depends on `bend-build`.
The Bend sources are `examples/proven/bend/`. A broken law fails that
build script; `cargo test -p bend-build` covers that gate with a temporary
file.

## License

MIT. See `LICENSE-MIT`.
