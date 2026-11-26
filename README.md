# payments-engine

A toy payments engine. It reads a CSV of deposits, withdrawals, disputes,
resolutions and chargebacks, and writes the resulting client accounts as CSV.

```sh
cargo run -- transactions.csv > accounts.csv
cargo test
```

Accounts go to stdout. Every skipped row goes to stderr with the reason, so the
output file stays clean.

## Layout

| Module | Responsibility |
|---|---|
| `money` | Fixed-point `Amount` (positive, from input) and `Balance` (signed, in accounts) |
| `transaction` | `ClientId`, `TransactionId` and the validated `Transaction` |
| `dispute` | The dispute state machine |
| `account` | One client's balances and the operations allowed on them |
| `engine` | Applies transactions to accounts and remembers deposits |
| `csv_io` | Streaming CSV reader and account writer |
| `lib` | `process(input, output, report_skipped)`, which ties the above together |

The engine knows nothing about CSV or files. `process` accepts any `Read` and
`Write`, and `main` only wires a file and stdout into it.

## Correctness

### Enforced by types

- **Money is never a float.** Values are whole ten-thousandths in an `i64`,
  parsed directly from text. All arithmetic is checked, so an overflow is a
  rejected transaction instead of a wrapped balance.
- **`Amount` and `Balance` are different types.** An `Amount` can only be built
  by parsing and is always greater than zero. A `Balance` may be negative. The
  two cannot be mixed up, and a negative deposit cannot be represented.
- **`ClientId` and `TransactionId` are newtypes**, so they cannot be swapped.
- **Rows are parsed, not validated.** A raw CSV row is converted once into a
  `Transaction`. After that point a deposit without an amount does not exist.
- **Dispute transitions live in one exhaustive `match`** (`DisputeState::after`).
  It has no wildcard arm, so a new state or action will not compile until every
  combination is handled.
- **`total` is derived** from `available + held` and never stored, so the three
  figures cannot disagree.
- **A rejected transaction changes nothing.** Each operation computes every new
  value before assigning any of them.

### Enforced by tests

- Unit tests sit beside each module: amount parsing and formatting, the state
  machine, account operations, engine scenarios and CSV reading and writing.
- `tests/end_to_end.rs` runs the fixtures in `tests/fixtures` through both the
  library and the compiled binary. The expected outputs were worked out by hand.

## Assumptions

| Situation | Behaviour |
|---|---|
| Dispute on a withdrawal | Ignored. Only deposits can be disputed, since holding funds only makes sense for money that came in. |
| Dispute on a deposit that was already spent | Allowed. `available` becomes negative: the client owes the money. |
| Dispute, resolve or chargeback naming a client that does not own the deposit | Ignored. |
| Dispute on a resolved deposit | Allowed. A resolve returns the deposit to its undisputed state. |
| Any action on a charged-back deposit | Ignored. A chargeback is final. |
| Deposit or withdrawal on a locked account | Rejected. |
| Dispute, resolve or chargeback on a locked account | Processed. Locking an account does not stop the partner from reversing earlier deposits. |
| Deposit reusing an existing deposit's transaction id | Rejected. |
| Withdrawal reusing a transaction id | Not detected. See [Efficiency](#efficiency). |
| Amount that is zero, negative or has more than four decimal places | The row is skipped. Silently rounding money would be worse. |
| Amount with a missing side of the decimal point, such as `1.` or `.5` | Read as `1.0` and `0.5`. |
| Row with more fields than the header, such as `deposit,1,1,1,000.00` | The row is skipped. A thousands separator must not turn 1,000 into 1. |
| Row with no amount column at all, such as `dispute,1,1` | Accepted for disputes, resolves and chargebacks. |
| Amount on a dispute, resolve or chargeback row | Ignored. |
| Rejected withdrawal for an unseen client | The client appears in the output with zero balances. |
| Unreadable row for an unseen client | No client is created. |

Output rows are ordered by client id and always show four decimal places.

## Error handling

- A row that cannot be parsed, or a transaction the engine rejects, is reported
  on stderr and skipped. One bad row from a partner should not discard the rest
  of the file.
- Failing to open or read the input, or to write the output, stops the program
  with a non-zero exit code.
- There is no `unsafe` code and no `unwrap`, `expect` or `panic!` outside tests.

## Efficiency

- The input is streamed. One record is held in memory at a time, in a buffer
  that is reused for every row.
- Memory grows with the number of clients (at most 65,536) and the number of
  **accepted deposits**, because a deposit can be disputed at any later point.
  Each stored deposit occupies about 25 bytes of hash map (a 4-byte id, a
  16-byte record and a control byte, with padding). Withdrawals are never
  stored, which is why a reused withdrawal id cannot be detected.
- Measured on a laptop with a release build: 5 million rows, of which 4.4
  million are stored deposits, take about 2.6 seconds and peak at 420 MB. The
  peak is well above 25 bytes per deposit because the map keeps spare capacity
  and briefly holds two copies of its table while growing.
- A file with billions of deposits would outgrow memory. The deposit map is
  private to `Engine`, so it could be replaced by a disk-backed store without
  touching any other module.

### Serving many concurrent streams

`Engine::apply` is synchronous and does no IO, so it can sit behind any
transport. Accounts are independent of each other and a dispute always names
its client, so a server could shard engines by client id, give each shard a
single-threaded owner, and route transactions from every connection to the
right shard over a channel. That keeps per-client ordering without locks. It is
not built here because the required interface is a single file.
