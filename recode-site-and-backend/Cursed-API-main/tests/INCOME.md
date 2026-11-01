# Income and promo integration

`income.rs` maintains kopek amounts and basis-point rates (10000 = 100%).
The public checkout still sells exactly one Recode subscription. RECODE is an
unlimited 10% code. Media defaults: buyer discount 15%, owner share 15%.
UID 622 receives 20%, UID 19 receives 5%; UID 0 receives the residual.
Discount is applied before processor fees; shares are calculated after fees.
Rounding is half-up to kopeks, with the last share capped to the remaining
kopeks so a 100% allocation cannot overdraw the order.

Conditions are copied at order creation. Code rename, disable or rate changes
do not rewrite existing orders. Previous names stay reserved to the same code.
Only UID 0 can change rates, create assigned codes and record manual payouts.
A media-role holder may create one personal code and rename it. An assigned
owner may rename their existing code. Readers receive only their own entries.
A recipient may hold multiple roles; their ledger amounts are combined.

`GET /api/pay/quote` quotes the discount. Checkout revalidates the code and binds
the displayed expected amount. `GET /api/income` provides aggregates for the
selected period and the last 100 orders. `GET /api/income/access` is a safe
capability query for any signed-in user. Mutation routes require same origin.

A signed new-order paid callback is additionally reconciled against the
provider's authenticated payment endpoint. The actual `payment_method` and
`platform_fee_percent` are used; if the latter is not yet present, the agreed
tariff for the provider-confirmed method is used (SBP 9%, crypto/xrocket 3%,
CryptoBot 4%). CryptoBot is not enabled at checkout. Missing/unrecognized fee
information yields a retryable response, without granting access or booking
income. Access, payment ledger and shares commit in one SQLite transaction.
Duplicate paid callbacks are idempotent; refunded/chargeback events reverse
only that order. A paid replay cannot restore reversed shares or access.
Pre-existing orders without a snapshot keep their previous payment lifecycle.

Manual payouts record transfers already made outside this system. They never
call a provider's payout API. Requests have an idempotency ID and cannot exceed
the recipient balance. A refund after payout produces a negative balance,
offset by future earnings. Amounts displayed are ruble accounting amounts,
not the actual USDT wallet balance or exchange-rate proceeds.

Validation on a Linux build based on the currently deployed sources:

```
cargo test pay:: -- --test-threads=1
INCOME_TEST_BINARY=/absolute/path/cursed-api INCOME_TEST_DIR=/private/tmp python3 tests/income_smoke.py
ROLLYPAY_TEST_BINARY=/absolute/path/cursed-api ROLLYPAY_TEST_DIR=/private/tmp python3 tests/rollypay_smoke.py
```

HTTP smoke creates its own database and mock credentials; it never touches the
production DB or sends a real provider payment request. Synthetic ledger rows
are confined to that isolated database. Rust tests exercise actual settlement,
rollback, duplicate notification and reversal code. Real money is not spent.

Payment recovery (27 October): provider decimal strings may have up to 18
fractional digits, provided digits beyond kopeks are zero. No sub-kopeck
rounding is allowed. A regression covers the actual `299.00000000` response.
A background task reconciles up to 10 due, unfulfilled orders per minute for
the last seven days. Only authenticated provider data can issue entitlement.
Unfinished full-price orders from the earlier checkout receive a snapshot on
upgrade; already fulfilled historic orders are not backfilled. This lets the
first formerly stuck purchase participate in the agreed admin shares.
