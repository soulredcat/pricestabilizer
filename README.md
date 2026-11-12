# pricestabilizer

A chain-agnostic **hybrid growth market-making engine** written in Rust.

The engine watches a TOKEN/QUOTE market, decides where liquidity should sit,
measures the market against a long-run growth benchmark, and switches to a
latched protection mode when something abnormal happens. It never knows which
chain or venue it is running on. Everything chain-specific lives in a connector
outside this repository that turns its market into generic snapshots.

> **Status: v0.2, paper trading.** Decisions are executed against a paper
> venue with a double-entry ledger, a risk gate and loss limits. No real
> venue adapter ships in this repository. Every threshold in the example
> config is an uncalibrated starting point for simulation.

## Principles

- **Chain-agnostic.** The engine's only input is a `MarketSnapshot`
  (timestamp, spot price, quote depth, quote volume, quote USD price). It has no
  addresses, ticks, decimals or SDKs.
- **Venue-neutral liquidity.** Liquidity is planned as a *price band*. A venue
  adapter maps a band to a concentrated-liquidity position or to a ladder of
  resting orders, so AMMs and order books are equally supported.
- **The growth line is a benchmark, not an order.** `P0 × (1 + g)^weeks`
  measures performance. Being below it only ever opens an *accumulation
  review*. It never buys by itself.
- **Separate prices.** Spot (what the market shows), TWAP reference (what
  decisions use, and only when trusted) and target (the benchmark) are never
  mixed.
- **Protection latches.** A stablecoin depeg, an extreme single move, a depth
  collapse, a loss limit, a ledger/venue mismatch or repeated venue failures
  trips protection, and only an operator may resume.
- **Integer books.** The ledger is double-entry in integer base units with
  configurable decimals, and it is reconciled with the venue every snapshot.

## Pipeline

```text
snapshot ─► price assessor ─► regime classifier ─► mode controller ─► band keeper
            (TWAP + trust)    (trend, volatility,   (review / market   (open / hold /
                               emergencies)          making / protect)   reposition)
                                                          ▲                  │
                                               alerts,    │                  ▼
                                               band held  └──── executor: risk gate ─► venue
                                                                 ledger, reconciliation, PnL
```

## Layout

```text
src/
├── main.rs                      CLI entry point
├── app/                         run loop, per-feed trading session, error type
├── config/                      TOML config: root, engine sections, feed, validation
├── market/
│   ├── snapshot.rs              the one market shape the engine understands
│   └── feed/
│       ├── stream.rs            opens configured feeds as snapshot streams
│       ├── jsonl.rs             snapshots from an external connector (JSON lines)
│       └── scenario/            synthetic stress scenarios: kind, shape, generator, PRNG
├── engine/
│   ├── growth/                  growth benchmark curve
│   ├── price/                   TWAP window, trust checks, assessor
│   ├── regime/                  market window, regime vocabulary, classifier
│   ├── mode/                    operating modes and the latching controller
│   ├── liquidity/               price band, liquidity intents, band keeper
│   └── strategy/                tick decision, execution feedback, pipeline
├── execution/                   band funding, risk gate, booking, executor
├── treasury/                    accumulation proposals and the treasury desk
├── operator/                    operator commands and the followed command file
├── portfolio/                   asset units, holdings, double-entry ledger, PnL
├── venue/
│   ├── adapter.rs               the Venue trait every market implements
│   ├── order.rs                 orders, fills, wallets, venue errors
│   └── simulated/               paper venue and its band position
└── report/                      run summary, text, live event log, ledger export
```

## Running

```bash
cargo run --release -- config/simulation.example.toml
```

The example config replays ten stress scenarios (steady bull and bear,
sideways, flash crash, pump and retrace, low liquidity, liquidity exit and
inflow, stablecoin depeg, data gap), each on a fresh engine, and prints one
summary per scenario.

## Feeding real market data

Point the feed at a JSON-lines file written by any connector:

```toml
[feed]
source = "jsonl"
path = "snapshots.jsonl"
```

One snapshot per line. Extra fields are ignored, so connectors may attach
their own metadata:

```json
{"timestamp": 1767225600, "spot_price": 1.0, "quote_depth": 50000.0, "quote_volume": 210.5, "quote_usd_price": 1.0}
```

Timestamps must strictly increase. Prices must be positive and depth and
volume non-negative, or the run stops with the offending line.

## Operator commands

Set `[operator] commands_path` to a JSON-lines file. The engine follows it
like a log: lines appended while it runs take effect once a snapshot reaches
their `at` time.

```json
{"at": 1767312000, "command": "pause"}
{"at": 1767398400, "command": "resume"}
{"at": 1767229300, "command": "approve", "proposal": 1}
{"at": 1767229300, "command": "reject", "proposal": 2}
```

- `pause` latches protection at once.
- `resume` leaves protection only if the market is no longer in an
  emergency; it acknowledges losses so far (the drawdown peak and daily
  baseline restart). A broken ledger is never cleared by a resume.
- Treasury proposals are announced on stderr, for example
  `[feed @ 1767229200] treasury_proposed #1`. Only an `approve` naming that
  number lets the treasury buy, and only while the price is trusted and the
  engine is not in protection. Unapproved proposals expire.

## Roadmap

1. ~~Inventory and ledger in integer base units.~~ Done.
2. ~~Band position simulation with fees, costs and PnL.~~ Done.
3. ~~Risk limits gating every order; loss limits trip protection.~~ Done.
4. ~~Treasury budget and accumulation proposals behind operator approval.~~ Done.
5. Venue adapter interface: quote, simulate, execute, reconcile.
6. ~~Operator controls: pause, resume, approve, reject.~~ Done.

## Repository rules

- Rust only. Everything in the repository is written in English.
- No chain-specific code, config or data is ever committed.
- Files stay small: aim for 200 lines, 400 at most (enforced by `tests/source_layout.rs`).
- Folders nest from general to specific, and file names say what they hold.
- No dead code: the crate denies every `unused` lint, so unused items fail the build.
