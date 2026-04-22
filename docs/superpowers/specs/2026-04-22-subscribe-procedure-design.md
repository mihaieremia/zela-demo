# `subscribe` procedure — design

Single-shot smoke test that the host's `rpc-subscription` WIT resource works
end-to-end through `zela-std`'s `PubsubClient`. Subscribes to Solana slot
updates, collects N slot infos, returns. No transactions, no signing, no
program-specific setup — designed to be the cheapest possible probe of the
subscription path.

## Wire contract

**Input** (JSON):

```json
{ "count": 5 }
```

| Field   | Type    | Required | Constraint        |
|---------|---------|----------|-------------------|
| `count` | `usize` | yes      | `1..=50`          |

The hard max of 50 caps wall time at ~20 s (Solana slots fire every ~400 ms),
which is well under any executor-side timeout and bounds the wedge surface
described by the codex review of `poll_await`.

**Output** (JSON):

```json
{
  "collected": 5,
  "slots": [
    { "slot": 414812345, "parent": 414812344, "root": 414812320 },
    { "slot": 414812346, "parent": 414812345, "root": 414812321 }
  ]
}
```

`collected` reports the actual count returned. It can be less than the
requested `count` when the stream errors mid-collection — see Errors below.

## Behavior

```
1. Validate `count` ∈ 1..=50; reject otherwise as RpcError code 400.
2. PubsubClient::new().slot_subscribe() — fail-fast on subscribe error.
3. Take next `count` items from the stream via futures_util::StreamExt::next.
4. On stream Err mid-collection: stop, return what was collected.
5. Drop the stream so the host closes the subscription.
6. Return { collected, slots }.
```

## Errors

| Condition                  | Code  | Message                                    |
|----------------------------|-------|--------------------------------------------|
| `count` out of range       | `400` | `"count must be in 1..=50"`                |
| Subscription failed        | `1`   | host's `PubsubClientError` message         |
| Stream Err on first item   | `1`   | `"stream error before any data"`           |
| Stream Err after some data | n/a   | success path; `collected < count` signals  |

The "partial-success on mid-stream error" choice trades cleanliness for
diagnostic value — a caller seeing `collected: 3` of `count: 5` knows the
subscription started fine and broke after a few items, which is more useful
for debugging the executor than a blanket failure.

## Crate layout

New `subscribe/` workspace member, mirroring the existing demos:

```
subscribe/
├── Cargo.toml          # zela-std workspace dep + serde + log
└── src/
    └── lib.rs          # ~60 lines: Input, Output, CustomProcedure impl
```

Workspace and CI updates:

- `Cargo.toml` `[workspace] members` → add `"subscribe"`
- `.github/workflows/deploy.yml` → add `subscribe` to the `procedure` choice list
- `.github/workflows/deploy-on-push.yml` → add `subscribe` to the matrix
- `README.md` → document the procedure under section 4

## Out of scope

- No `commitment` config exposure (defaults to whatever `RpcClient::new()` uses)
- No filtering of slot updates by parent/root delta
- No metrics aggregation across the collected slots
- No timeout configuration (the count clamp serves as the implicit timeout)

## Dependencies

- `zela-std` workspace dep (already present)
- `serde` workspace dep (already present)
- `log` workspace dep (already present)
- `futures-util` for `StreamExt::next` — pulled transitively via `zela-std::rpc_client::*` re-export, no new direct dep needed

## What this exercises

- `Subscription::new` over `rockawayx:zela:zela-host.rpc-subscription`
- `SubscriptionStream::poll_next` repeatedly through `poll_await`
- `Drop` releasing the host-side subscription
- The full request/response flow under a procedure that holds the host
  for multiple seconds (unlike the existing demos which return in <1 s)
