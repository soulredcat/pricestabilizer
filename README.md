# pricestabilizer

A chain-agnostic **hybrid growth market-making engine** written in Rust.

The engine watches a TOKEN/QUOTE market, decides where liquidity should sit,
measures the market against a long-run growth benchmark, and switches to a
latched protection mode when something abnormal happens. It never knows which
chain or venue it is running on. Everything chain-specific lives in a connector
outside this repository that turns its market into generic snapshots.

> **Status: v0.4, ready for shadow runs.** Decisions are executed through a
> risk gate against a venue, every order carries a guard (minimum out,
> deadline, price drift, cost cap, idempotency key), the double-entry ledger
> is reconciled every snapshot, and all safety state survives restarts. The
> repository ships a paper venue and a chain-agnostic connector protocol; no
> chain connector ships here. Every threshold in the example configs is an
> uncalibrated starting point.

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
  collapse, a loss limit, a ledger/venue mismatch, repeated venue failures,
  a fill that does not match its order, or more orders in a day than allowed
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
├── execution/                   order runner, guards, fill checks, risk gate, band mover, executor
├── treasury/                    accumulation proposals and the treasury desk
├── operator/                    operator commands and the followed command file
├── portfolio/                   asset units, holdings, double-entry ledger, PnL
├── state/                       what survives a restart, and its atomic file
├── venue/
│   ├── adapter.rs               the Venue trait every market implements
│   ├── order.rs                 orders, fills, wallets, venue errors
│   ├── external/                connector protocol, process client, paper server
│   └── simulated/               paper venue and its band position
└── report/                      run summary, text, live event log, ledger export
```

## Running

```bash
cargo run --release -- config/simulation.example.toml
cargo run --release -- check-config config/live.example.toml   # validate only
cargo run --release -- --version
```

The simulation config replays ten stress scenarios (steady bull and bear,
sideways, flash crash, pump and retrace, low liquidity, liquidity exit and
inflow, stablecoin depeg, data gap), each on a fresh engine, and prints one
summary per scenario.

## Feeding real market data

Point the feed at a JSON-lines file written by any connector, or pipe the
same lines in live:

```toml
[feed]
source = "jsonl"        # a recorded file, read to its end
path = "snapshots.jsonl"
```

Live, either pipe the lines in, or let a connector append them to a file
the engine follows (the natural setup with containers):

```toml
[feed]
source = "follow"       # or "stdin" (without path and poll_millis)
path = "/data/snapshots.jsonl"
max_snapshot_age_secs = 120
poll_millis = 500
```

```bash
my-connector --snapshots | pricestabilizer live.toml   # with source = "stdin"
```

One snapshot per line. Extra fields are ignored, so connectors may attach
their own metadata:

```json
{"timestamp": 1767225600, "spot_price": 1.0, "quote_depth": 50000.0, "quote_volume": 210.5, "quote_usd_price": 1.0}
```

Timestamps must strictly increase, prices must be positive, and depth and
volume non-negative. A line that breaks these rules, or does not parse, is
reported on stderr, counted under `skipped` in the summary and never acted
on; the engine keeps running. Live feeds also skip snapshots further than
`max_snapshot_age_secs` from the wall clock.

## Writing a venue connector

A connector executes orders on one concrete market, in any language. Set

```toml
[venue]
kind = "external"
command = ["node", "connector.js"]
timeout_secs = 30       # an unanswered call counts as a connector failure
send_orders = false     # kill switch: read-only until you flip it
```

and the engine starts that process and speaks JSON lines on its stdin and
stdout, one request and one response per line, in order:

| `method`   | request fields          | `ok` answer                                    |
|------------|-------------------------|------------------------------------------------|
| `observe`  | `market` (snapshot)     | `null`                                         |
| `execute`  | `order`, `guard`, `market` | fill: `received`, `spent`, `fees_claimed`, `action_cost`, `swap_cost_quote` |
| `balances` | `wallet`                | holding: `token`, `quote`                      |
| `position` | `market`                | `token`, `quote`, `unclaimed_fees`, or `null`  |

```text
→ {"id":1,"method":"balances","wallet":"operational"}
← {"id":1,"ok":{"token":"4000000000000","quote":"5000000000"}}
→ {"id":2,"method":"execute","order":{"type":"remove_band"},"market":{…}}
← {"id":2,"error":"no_position"}
```

Orders are `swap` (`wallet`, `sell`, `units`, `min_received`), `add_band`
(`band` with `lower`/`upper` prices, `token_units`, `quote_units`) and
`remove_band`. Base units are decimal **strings**.

Every `execute` carries a `guard` the connector must honour exactly, or
refuse the order without executing anything:

| guard field        | the connector must                                           |
|--------------------|--------------------------------------------------------------|
| `order_id`         | answer a repeated id with the original result, never execute twice |
| `deadline`         | refuse with `expired` after this Unix time                   |
| `reference_price`, `max_price_drift` | refuse with `price_moved` when the live price drifted further |
| `max_action_cost`  | refuse with `cost_too_high` when network costs would exceed it |
| `min_received` (swap) | refuse with `slippage_exceeded` rather than accept less (use the venue's own min-out protection) |

Error codes: `insufficient_operational`, `insufficient_treasury`,
`no_position`, `position_exists`, `no_liquidity`, `expired`, `price_moved`,
`slippage_exceeded`, `cost_too_high`. Anything else, a malformed answer or
no answer within `timeout_secs` counts as a connector failure; repeated
failures trip protection. Every fill is checked against its order, and a
fill that does not match (for example less than `min_received`) trips
protection as an `execution_anomaly`. Connectors must log to stderr: stdout
carries only protocol lines.

Swaps that rebalance the band are anchored to spot; treasury buys are
anchored to the TWAP, so their `min_received` and `max_price_drift` refuse a
buy into a spike that the TWAP has not confirmed. The engine calls `observe`
on every snapshot: on venues with resting orders (order books), a connector
should cancel them when no call has arrived for a few snapshot intervals
(a dead-man switch), so a stopped engine never leaves stale quotes behind.

The binary contains a reference connector, the paper venue served over this
protocol. A real connector must answer the same calls the same way:

```bash
pricestabilizer paper-connector config/simulation.example.toml
```

## Stabilization options

All in `[liquidity]` unless noted. Defaults in the example configs are
starting points; calibrate them per market.

| option | effect |
|---|---|
| `range_sigmas`, `range_horizon_days`, `min_half_width`, `max_half_width` | band width follows realised volatility: tight when calm, wide when wild |
| `band_capital_share` | share of operational value placed in the band; it grows with the capital actually held |
| `high_volatility_capital_share` | smaller share while the regime is high volatility |
| `width_tolerance`, `min_reposition_interval_secs` | hysteresis and cooldown against repositioning churn |
| `rebalance_tolerance` | inventory gaps below this share of the band are not worth a swap |
| `inventory_skew` (experimental) | lifts the band when token-heavy (the excess sells as price rises) and lowers it when quote-heavy |
| `pool_fee_rate`, `min_fee_to_lvr` | economic gate: no new band while expected fees fall below this multiple of the loss to arbitrage |
| `[risk]`, `[regime]` | loss limits, exposure, reserve, depeg, extreme-move and depth-collapse protection |
| `[treasury]` | operator-approved accumulation only |

**What a band costs.** While the price is in range, arbitrage takes
`σ² · L · √p / 4` per day from a band of liquidity `L` (loss-versus-
rebalancing), and the band earns roughly `fee · volume · (its depth / pool
depth)`. Their ratio does not depend on the band's width or size:

```text
fees / LVR = 4 · fee · daily volume · (1 − 1/√(1 + m)) / (depth · σ²)
```

with `m` the feed's depth reference move. Every run reports it as
`economics`: below 1, providing liquidity costs the project money, and that
cost is the price of the depth it adds. In the built-in scenarios
(turnover about 1.2× depth per day, 3–4% daily volatility) it sits at
0.1–0.2, which matches the negative results against holding. Markets with
high turnover relative to depth come out far better.

**Measured, not assumed.** On the built-in scenarios `inventory_skew = 0.5`
cut swap costs where the price came back (pump and retrace) but did slightly
worse than no skew in steady trends, so it ships off. Change a default only
when it also wins out of sample.

## Operator commands

Set `[operator] commands_path` to a JSON-lines file. The engine follows it
like a log: lines appended while it runs take effect once a snapshot reaches
their `at` time.

```json
{"at": 1767312000, "command": "pause"}
{"at": 1767398400, "command": "resume"}
{"at": 1767229300, "command": "approve", "proposal": 1}
{"at": 1767229300, "command": "reject", "proposal": 2}
{"at": 1767400000, "command": "reconcile"}
```

- `pause` latches protection at once.
- `resume` leaves protection only if the market is no longer in an
  emergency; it acknowledges losses so far (the drawdown peak and daily
  baseline restart). A broken ledger is never cleared by a resume.
- `reconcile` adopts the venue's wallet balances in the ledger after
  transfers made outside the engine (a `ledger_mismatch` stops trading until
  then); send `resume` afterwards.
- Lines that do not parse are reported on stderr and skipped.
- Treasury proposals are announced on stderr, for example
  `[feed @ 1767229200] treasury_proposed #1`. Only an `approve` naming that
  number lets the treasury buy, and only while the price is trusted and the
  engine is not in protection. Unapproved proposals expire.
- An approved proposal is bought in slices of at most `slice_quote`, at
  least `min_slice_interval_secs` apart, never more in a UTC day than
  `max_daily_volume_share` of the market's estimated daily volume, and each
  slice is anchored to the TWAP. With `only_outside_own_band = true` slices
  wait while the price sits inside the project's own band, so the treasury
  never trades against the project's own liquidity. An approval that buys
  nothing for `proposal_ttl_secs` expires; a slice that fails closes the
  proposal; `reject` cancels whatever is left.

## State and restarts

```toml
[state]
dir = "state"           # one <feed>.state.json per feed, saved every snapshot
```

The state file keeps everything a restart must not reset: a latched
protection, the drawdown peak and the day's opening equity, ledger balances,
order numbering, the treasury budget spent and its proposal numbers, how far
the operator command file was read, and the paper venue's wallets. It is
written atomically after every snapshot, so the process can be killed at
any moment. On restart, snapshots up to the last one handled are refused,
commands already applied are never applied again, and price windows warm up
afresh before anything is traded. A state file that exists but cannot be
read stops the engine instead of starting fresh. It is required when an
external venue sends orders.

With `[report] ledger_dir`, every ledger entry is appended to a JSON-lines
journal as soon as it is booked; a resumed session appends to the same
journal.

## Running with Docker

```bash
docker build -t pricestabilizer .
docker run --rm -v "$PWD/config:/config:ro" pricestabilizer check-config /config/engine.toml
```

`deploy/compose.example.yaml` runs one engine with `restart: unless-stopped`,
the config mounted read-only at `/config` and a `data/` volume at `/data`
for the state directory, the ledger journal, the operator command file and
the snapshot file a connector appends to. Start from
`config/live.example.toml`. The image carries no venue connector: build
your own image `FROM pricestabilizer`, add the connector, and point
`[venue] command` at it.

## Going live

Run these stages in order and move on only when the previous one is clean.

1. **Calibrate.** Record the target market's snapshots and tune every
   threshold (see below). Keep only changes that also win out of sample.
2. **Connector.** Implement the protocol and honour every guard: enforce
   `min_received` with the venue's own minimum-out protection, refuse late
   or drifted orders, answer repeated `order_id`s from memory, and prefer a
   private submission route where the chain offers one. The hot wallet
   holds only operational funds; the treasury wallet is controlled
   separately; token allowances are exact and short-lived, never unlimited.
3. **Read-only.** Run with `send_orders = false` and a state directory.
   Check that balances reconcile, that the event log makes sense and that
   withheld orders are the ones you expect.
4. **Shadow.** Paper-trade on live data (the paper venue with a live feed)
   for several days, including at least one volatile day.
5. **Pilot.** `send_orders = true` with a small fraction of the capital and
   limits tighter than the final ones. Watch the event log and the journal
   daily; back up `data/`.
6. **Scale** only after the pilot's books, fees and costs match
   expectations.

Never: trade between wallets the project controls, place orders meant to
be cancelled, or trade to move a reported price. Disclose the market-making
arrangement and the buyback policy, and keep the ledger journal (it is the
audit trail).

## Roadmap

1. ~~Inventory and ledger in integer base units.~~ Done.
2. ~~Band position simulation with fees, costs and PnL.~~ Done.
3. ~~Risk limits gating every order; loss limits trip protection.~~ Done.
4. ~~Treasury budget and accumulation proposals behind operator approval.~~ Done.
5. ~~Venue connector protocol with a reference paper connector.~~ Done.
6. ~~Operator controls: pause, resume, approve, reject.~~ Done.
7. ~~Order guards, persistent state, reconciliation, kill switch, live
   follow feed, container image.~~ Done.
8. Calibrate every threshold on recorded market data, then shadow-run
   against a testnet connector before any pilot.

## Calibrating on recorded data

Thresholds belong to a market, not to the engine. Record that market's
snapshots, then for each candidate configuration:

1. Replay it and compare equity with the `hold` benchmark the report prints.
2. Re-run the best candidates on data they were not chosen on (another
   period, the other half of the record, and the built-in stress scenarios).
3. Keep a change only when it wins out of sample too. A parameter that wins
   on one path, such as a single pump and retrace, is luck, not an edge.

Keep recordings, sweeps and market-specific configs out of this repository
(the git-ignored `local/` folder exists for that).

## Repository rules

- Rust only. Everything in the repository is written in English.
- No chain-specific code, config or data is ever committed.
- Files stay small: aim for 200 lines, 400 at most (enforced by `tests/source_layout.rs`).
- Folders nest from general to specific, and file names say what they hold.
- No dead code: the crate denies every `unused` lint, so unused items fail the build.
