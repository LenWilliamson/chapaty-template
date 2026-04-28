# Rust Vibe Rules (Coding Style)

> **FOR LLMs:** Your goal is to let traders with zero Rust background read, understand, and safely edit the code in `src/agents/<name>/agent.rs`.
> **Speed of the user's iteration cycle beats the speed of the user's hot loop.** Simple is better than clever.

## The Performance Philosophy

As an AI, you must anchor your Rust generation to the [Rust Book](https://doc.rust-lang.org/book/) best practices, but with a specific bias for this framework:

- **DO Optimize Algorithmically (Big-O):** Avoid $O(n^2)$ or $O(n!)$ logic. Do not iterate over the entire price history on every single tick. Use rolling windows, stateful variables, or the provided TA indicators (`StreamingSma`, etc.) to maintain $O(1)$ or $O(\log n)$ step times.
- **Do NOT Over-Engineer Syntactically:** Write flat, readable code. Do not introduce generic lifetimes, complex trait bounds, or zero-copy parsing micro-optimizations.

## 1. Embrace Newtypes (No Raw Floats)

Chapaty uses strong domain types. Do not use raw `f64` for prices or sizes in your function signatures or agent state unless it is a pure mathematical multiplier (like a risk factor). Use `Price`, `Quantity`, `Tick`, and `Volume`. Unpack them (`price.0`) to do math, and repackage them immediately.

**BAD:** `fn calculate_tp(entry: f64) -> f64 { ... }`

**GOOD:** `fn calculate_tp(entry: Price) -> Price { ... }`

## 2. No Lifetimes in User Code

Never write `<'a>` in strategy code. The chapaty `Observation<'env>` lifetime is already elided at the API boundary. If you feel you need a lifetime, **clone instead**. (See [Common Rust Lifetime Misconceptions](https://github.com/pretzelhammer/rust-blog/blob/master/posts/common-rust-lifetime-misconceptions.md)).

**BAD:** `fn compute<'a>(view: &'a MarketView<'a>) -> &'a Price { ... }`

**GOOD:** `fn compute(view: &MarketView) -> Price { view.something().copied().unwrap_or_default() }`

## 3. `.clone()` is Not a Sin

`chapaty` is fast because of its batching and Rayon parallelism, not because user code is zero-copy. Cloning `Ohlcv`, `Price`, `Duration`, or `AgentIdentifier` costs nanoseconds in a loop that takes microseconds. **Clone freely; the borrow checker is not your enemy here.** Avoid heavy allocations inside `act()` if possible, but do not sacrifice readability for it.

## 4. Enums Over Booleans (State Machines)

Code is documentation. Encode strategy state as an enum; never use `is_armed: bool` or `has_seen_news: bool`. This prevents invalid overlapping states.

**BAD:**

```rust
struct Agent { has_seen_news: bool, is_in_trade: bool }
```

**GOOD:**

```rust
#[derive(Debug, Copy, Clone, Default)]
enum Phase {
    #[default]
    AwaitingNews,
    PostNews { news_time: DateTime<Utc> }
}
struct Agent { phase: Phase }
```

## 5. No Panics or Accidental Crashes in the Hot Loop

Never use `.unwrap()` or `.expect()` inside the `act()` method. Market data is asynchronous; things will occasionally be missing.

**WARNING ON THE `?` OPERATOR:** Using `?` on a data-fetching method (like `try_resolved_close_price`) will propagate the error to the engine and **crash the simulation**. Handle expected missing data gracefully using `match` or `if let` and return `Ok(Actions::no_op())`. Reserve `?` or `Err(...)` ONLY for fatal logic bugs that _should_ halt the backtest.

## 6. Always Return `ChapatyResult<T>`

Agent methods and helpers that can fail must return `ChapatyResult<T>`.
For user-caused invalid input (e.g., bad parameters in the constructor), return:
`Err(AgentError::InvalidInput("...".to_string()).into())`

## 7. Struct-Init is the Constructor Style

Prefer plain struct initialization over a chain of `.with_foo().with_bar()` for one-off construction. The builder pattern is fine _inside_ the agent module when there are many optional parameters, but keep it minimal.

## 8. No `unsafe`, No `async fn` Inside the Agent

The engine handles async data streaming; the agent itself is synchronous. `act` is a standard `fn`, not `async fn`. Never use `unsafe`. If you think you need async data access inside the agent, you have chosen the wrong architectural pattern.

## 9. Logging (`println!` vs `tracing`)

For temporary, quick-and-dirty debugging of logic errors, using `dbg!` and `println!` is completely fine. However, **you MUST remove them before finalizing the agent or running grid searches**, as printing on every tick will severely bottleneck the CPU.

For permanent production logging, use `tracing::debug!` or `tracing::info!`, and limit logs to state transitions (e.g., "Entered Trade", "Phase Changed"). If the user wants to set up the tracing subscriber, direct them to `examples/logging.rs` in the core Chapaty repo.

## 10. One Agent = One File

Do not split `agent.rs` into multiple files unless the user explicitly asks. Keep the agent's state struct, helper `impl`, and `impl Agent` cleanly organized within a single `src/agents/<name>/agent.rs` file.

## 11. Test IDs with `..Default::default()`

For the small number of chapaty configuration structs that implement `Default`, use struct update syntax (`..Default::default()`) where applicable to keep boilerplate readable.

## 12. Comments: Explain the _Why_, Not the _What_

Do not narrate obvious code. Use comments to explain business rules, magic numbers, or financial logic that aren't obvious from the syntax.

**BAD:** `// Increment the trade counter`

**GOOD:** `// Risk factor 0.89 comes from the Q3 2024 optimization sweep`
