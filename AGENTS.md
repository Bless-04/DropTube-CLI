# AGENTS.md

## 0. Prime Directive

This is a production-grade Rust codebase. Every change must be **safe, explicit, and boring**.
When in doubt, choose the version of the code a reviewer can understand in 10 seconds over the
version that is 5% faster or 3 lines shorter. Cleverness is a liability, not a virtue.

If a rule in this file conflicts with speed of delivery, the rule wins. Ask before breaking a rule.

---

## 1. Build, Test & Quality Gates

All of these must pass with **zero warnings** before any change is considered complete.

| Purpose | Command |
|---|---|
| Build (debug) | `cargo build` |
| Build (release) | `cargo build --release` |
| Run all tests | `cargo test --all-features --all-targets` |
| Run one test | `cargo test <test_name>` |
| Lint | `cargo clippy --all-targets --all-features -- -D warnings` |
| Format check | `cargo fmt -- --check` |
| Auto-format | `cargo fmt` |
| Dependency audit | `cargo audit` |
| License / advisory / duplicate check | `cargo deny check` |
| Unused dependencies | `cargo udeps --all-targets` |
| Public API diff (libraries) | `cargo semver-checks` |
| Undefined behavior check (only if `unsafe` is present) | `cargo +nightly miri test` |

If a tool above isn't installed, **stop and ask** before skipping the check — do not silently
treat it as passing.

---

## 2. Unsafe Code Policy (non-negotiable)

* Every crate root (`lib.rs` / `main.rs`) **must** contain:
  ```rust
  #![forbid(unsafe_code)]
  ```
  This is the default for every crate in this workspace.
* If a specific module genuinely requires `unsafe` (FFI, a hand-rolled data structure, a
  perf-critical hot path with a benchmark proving it's necessary), that module — and *only* that
  module — may downgrade to `#![deny(unsafe_code)]` at the module level, and the change **requires
  explicit human approval before being written**, not after.
* Every individual `unsafe` block must be preceded by a `// SAFETY:` comment that states, in
  plain language, which invariants the caller/compiler is relying on and why they hold. No
  `unsafe` block may be committed without one. `cargo clippy` should be run with
  `-W clippy::undocumented_unsafe_blocks` enabled to enforce this mechanically.
* `unsafe` is never used as a shortcut around the borrow checker to "make it compile." If you
  hit a borrow-checker wall, the fix is to restructure ownership (indices, `Rc`/`Arc`, splitting
  structs), not to reach for `unsafe`.
* Any `unsafe` code touching raw pointers or shared mutable state must have a corresponding
  `miri` test run and, where concurrency is involved, a `loom` model if feasible.

---

## 3. Error Handling

* **Never use `.unwrap()` or `.expect()`** in a path reachable from production code. The only
  exception is a value that is a mathematical or type-level invariant (e.g., indexing into a
  `const`-sized array with a compile-time-checked index) — and even then, prefer proving it via
  the type system over asserting it at runtime.
* **Library crates:** define errors with `thiserror`. Each fallible public function gets its own
  error enum or a well-scoped shared one — never a single catch-all `Error` enum for an entire
  crate. Errors are part of the public API; treat their shape with the same care as function
  signatures.
* **Binary/application crates:** use `anyhow` at the top level for context aggregation, but do
  not let `anyhow::Error` leak into library code that other crates depend on.
* Always attach context at the point of failure with `.context()` / `.with_context()` — the
  context should say *what operation was being attempted*, not restate the underlying error.
* Never swallow an error with `let _ = fallible_call();` — either handle it, propagate it with
  `?`, or log it explicitly with a stated reason for ignoring it.
* Error messages are for humans and logs, not control flow. Never `match` on the *string content*
  of an error to make a decision — encode that decision as a variant instead.

---

## 4. Panics & Control Flow

Treat "does this ever panic" as a first-class review question, separate from "does this return
`Result` correctly."

* Banned in production code paths: `unwrap()`, `expect()` (see §3 exception), `panic!()`,
  `unimplemented!()`, `todo!()`, unchecked slice indexing (`v[i]`) on external/user-controlled
  input, and integer arithmetic that can overflow on untrusted input (use `checked_*`,
  `saturating_*`, or `wrapping_*` explicitly, with a comment on which one and why).
* A function that can fail returns `Result<T, E>`. A function that legitimately cannot fail — as
  a proven invariant, not an assumption — may return `T` directly, but this must be justified.
* `todo!()` / `unimplemented!()` are permitted only in code that is not yet wired into any
  execution path (e.g., a stub in a PR the agent flags as incomplete), never in code presented as
  finished.
* Enforce this mechanically via `clippy.toml` / lint attributes (see §9).

---

## 5. Memory, Performance & Idiomatic Design

* Prefer borrowing (`&T`, `&str`) over owning (`T`, `String`) whenever a reference outlives the
  call it's needed for. Don't reach for `.clone()` to silence the borrow checker — understand
  *why* it's complaining first.
* Use `Cow<'_, str>` (or `Cow<'_, [T]>`) when ownership is conditionally required.
* Pre-allocate collections with `Vec::with_capacity()` / `HashMap::with_capacity()` whenever the
  final size is known or can be cheaply estimated.
* Favor iterator combinators over hand-rolled loops when it improves clarity — but not when it
  hurts it. A `for` loop with early `continue`/`break` is often more readable than a deeply
  chained iterator; choose readability over "looking functional."
* Keep functions to 5 or fewer parameters. Beyond that, group related parameters into a
  purpose-named struct (and prefer a builder or `..Default::default()` pattern over positional
  construction for structs with more than ~3 fields).
* No premature optimization: don't introduce `unsafe`, manual SIMD, or exotic data structures
  without a benchmark (`criterion`) demonstrating the standard-library approach is measurably
  insufficient for the actual workload.

---

## 6. Type System & API Design

Rust's type system is the primary defense against brittle code — use it aggressively.

* **Make illegal states unrepresentable.** Prefer an enum over a `bool` + comment, or over two
  fields that are only ever valid in combination. If two fields can never both be `Some`/`None`
  independently, they should probably be one enum instead.
* **Use the newtype pattern** for domain values that share a primitive representation (e.g.,
  `UserId(u64)` vs `OrderId(u64)`) so the compiler — not code review — catches transposed
  arguments.
* Avoid "stringly typed" APIs: no passing raw `&str` for things that have a fixed, known set of
  values (status codes, modes, kinds) — use an enum, ideally with `strum` for
  serialization/parsing if needed at the boundary.
* Public function signatures should make invalid calls a compile error, not a runtime `Result`,
  wherever that tradeoff is reasonable.
* Minimize `pub` surface area. Default to private; widen visibility only when a concrete caller
  outside the module needs it. `pub(crate)` is preferred over `pub` for anything not meant to be
  part of the crate's external API.
* Avoid `Box<dyn Trait>` / dynamic dispatch as a default — prefer generics and static dispatch
  unless there's a genuine need for runtime polymorphism (heterogeneous collections, plugin-style
  extension points) or it measurably reduces compile-time/binary bloat.

---

## 7. Concurrency & Async

* Default to `tokio` unless the project specifies otherwise.
* Ensure explicit, correct `Send`/`Sync` bounds on any type crossing a thread or task boundary —
  never `unsafe impl Send`/`Sync` without a documented, reviewed justification (see §2).
* Prefer the least powerful synchronization primitive that's correct: plain ownership > channels
  > `RwLock` (for read-heavy access) > `Mutex`. Don't reach for `Arc<Mutex<T>>` as a default
  crutch for shared state — first ask whether the state should be owned by a single task and
  accessed via message passing instead.
* Never block the async runtime: no `std::thread::sleep`, synchronous file/network I/O, or
  CPU-heavy computation inside an `async fn` without wrapping it in `tokio::task::spawn_blocking`.
* Bound your channels (`tokio::sync::mpsc::channel` with a real capacity) unless unbounded is a
  deliberate, documented choice — unbounded channels are a common source of unbounded memory
  growth under load.
* Cancellation safety matters: if a future can be dropped mid-execution (e.g., inside
  `tokio::select!`), document whether that's safe and why.

---

## 8. Dependency Management

* **Ask before adding any new crate to `Cargo.toml`.** When proposing one, state: what it's for,
  why the standard library or an existing dependency can't do it, its maintenance status
  (recent commits, download count), and its transitive dependency footprint.
* Prefer well-established, widely-audited crates over niche ones for anything touching parsing,
  crypto, or unsafe FFI.
* Pin a Minimum Supported Rust Version (MSRV) in `Cargo.toml` (`rust-version`) and don't use
  language features newer than it without updating it deliberately.
* Run `cargo deny check` to catch duplicate dependency versions, disallowed licenses, and known
  advisories — treat findings as build failures, not warnings to note and move past.
* Never vendor or hand-copy code from another crate instead of depending on it properly, and
  never depend on a git fork/branch without a comment explaining why the crates.io version won't
  work.

---

## 9. Testing & Verification

* **Every new function, module, and public API surface gets tests.** No exceptions for "trivial"
  functions — trivial functions are exactly where an agent-introduced typo hides.
* Structure tests with **Arrange-Act-Assert**, and name them for the behavior under test, not the
  function name alone (e.g., `rejects_empty_input_with_validation_error`, not `test_validate`).
* Cover the failure paths, not just the happy path — every `Err` variant your function can return
  should have a test that provokes it.
* Public library APIs get doctests (`/// # Examples` blocks) that actually compile and run via
  `cargo test --doc`.
* For parsers, deserializers, or anything handling untrusted/external input, prefer
  property-based tests (`proptest`) over a fixed list of hand-picked examples, and consider
  `cargo-fuzz` for anything parsing bytes from outside the program's control.
* No flaky tests: no `sleep`-based timing assumptions in async tests — use explicit
  synchronization (channels, `tokio::time::pause` + controlled advancement) instead.
* `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test` must both be clean
  before any change is presented as done.

---

## 10. Documentation

* Every `pub` item (function, struct, enum, trait, module) gets a doc comment explaining *what*
  it does and *why* it exists if that's not obvious from the name — not a restatement of the
  signature.
* Library crates enable `#![warn(missing_docs)]`; treat undocumented public items as a defect.
* Non-obvious invariants, "why not the obvious approach" decisions, and any `unsafe` justification
  live in comments at the point of use — not in a separate doc that will drift out of sync.
* Module-level (`//!`) docs explain how the module fits into the overall architecture, not just
  what's in the file.

---

## 11. Observability & Logging

* Use the `tracing` crate, not `println!`/`eprintln!`, for anything beyond a throwaway local
  debug session — and no `println!`/`dbg!` calls are committed to production files (see §12).
* Prefer structured fields (`tracing::info!(user_id = %id, "processed request")`) over
  interpolated strings, so logs remain queryable.
* Use spans to capture the lifetime of a logical operation (a request, a job), not just
  individual log lines, especially in async code where interleaving makes flat logs hard to read.

---

## 12. Security & Hygiene

* **Never hardcode secrets, API tokens, or credentials.** Load them via `std::env` or the
  `dotenvy` crate, and never commit a `.env` file with real values.
* **Never log sensitive data** (PII, passwords, tokens, full request bodies that may contain
  them). Wrap sensitive values in the `secrecy` crate so they can't be accidentally printed or
  serialized.
* No `println!` or `dbg!` macro calls in files that ship to production — these are development
  aids only and must be removed before a change is complete.
* Validate and sanitize all external input at the boundary (parsing, HTTP handlers, CLI args) —
  don't assume upstream validation; treat every module boundary as a trust boundary.

---

## 13. Anti-Pattern Blocklist

The agent must not do the following, even if it makes a change compile faster or "look" simpler:

* Adding `#[allow(dead_code)]`, `#[allow(clippy::...)]`, or `#[allow(unused)]` to silence a
  warning instead of fixing the underlying issue — an `#[allow]` is only acceptable with an
  inline comment explaining why the lint genuinely doesn't apply.
* Deleting or weakening a test to make CI pass.
* Introducing a global mutable singleton (`static mut`, ad-hoc `lazy_static` used as shared
  mutable state) instead of passing state explicitly or via proper synchronization.
* Cloning a large structure to avoid a borrow-checker error instead of restructuring ownership.
* Returning `Box<dyn std::error::Error>` from a library's public API (opaque, unmatchable,
  loses type information for callers).
* Catching a panic with `std::panic::catch_unwind` to paper over an unhandled error case instead
  of fixing the error handling.
* Copy-pasting a block of logic instead of extracting a function/trait, once it appears a second
  time.
* Making drive-by, unrelated refactors inside a change whose stated purpose is something else —
  keep diffs scoped to the task.

---

## 14. Agent Guardrails & Permissions

**Allowed without asking:**
Running local tests, `cargo check`, `cargo clippy`, `cargo fmt`, `cargo doc`, reading project
files, running `cargo audit` / `cargo deny check` in read-only/report mode.

**Ask first before:**
Adding or upgrading a dependency in `Cargo.toml`, widening an `#![forbid(unsafe_code)]` to
`#![deny(...)]` anywhere, executing destructive file changes (deletes, force-pushes,
`git reset --hard`), modifying CI/CD or deployment configuration, changing a public API's
signature in a way `cargo semver-checks` flags as breaking, or editing `Cargo.lock` by hand.

**Never do, even if asked casually in passing:**
Commit secrets, disable the CI quality gates in §1, or bypass this file's rules to "just get
something working" — flag the tension and ask instead.

---

## 15. Definition of Done

A change is not complete until all of the following are true:

- [ ] `cargo build --release` succeeds
- [ ] `cargo test --all-features --all-targets` passes, including new tests for new behavior
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` is clean
- [ ] `cargo fmt -- --check` is clean
- [ ] No new `unwrap()`/`expect()`/`panic!()` in production paths (§3, §4)
- [ ] No new `unsafe` without a `// SAFETY:` comment and prior approval (§2)
- [ ] No new dependency added without being flagged and approved (§8)
- [ ] Public items touched have doc comments (§10)
- [ ] No `println!`/`dbg!`/secrets left in the diff (§12)
- [ ] Diff is scoped to the stated task — no unrelated drive-by changes (§13)

---

## Appendix: Machine-Enforced Lints

Add to the crate root (`lib.rs` / `main.rs`):

```rust
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![warn(clippy::pedantic)]
```

Add to `Cargo.toml` (Rust 1.74+ workspace lints), so every member crate inherits these:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"

[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
undocumented_unsafe_blocks = "deny"
pedantic = "warn"
```

Each member crate then opts in with:

```toml
[lints]
workspace = true
```
