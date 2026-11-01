# RollyPay

API reference: https://docs.rollypay.io, checked 2026-10-27.

- Callback: `https://cursedclient.xyz/api/pay/rollypay/callback` (POST JSON).
- Return URLs: `/api/pay/ok` and `/api/pay/fail`, GET or POST; checkout adds our order ID.
- Authenticated checkout: POST `/api/pay/rollypay/create`; days 30/90/0, agree true,
  method sbp/crypto/xrocket. Prices are fixed on the server: 299/499/699 RUB.
- Status: GET `/api/pay/rollypay/status?id=...`, only for the order's owner.
- Environment: `deploy/rollypay.env.example`; install `rollypay.conf` in systemd drop-ins.
  Credentials stay on the server; the browser receives only the hosted checkout URL.
- `ROLLYPAY_ENABLED=1` enables checkout after callback setup. Missing settings fail closed.

Callbacks verify HMAC-SHA256 over the exact timestamp, dot and raw body. Validate
payment/order IDs, amount, currency, event and production environment before applying
any change. Test callbacks are acknowledged without creating entitlements. Status
polling uses the authenticated provider API and applies the same validation.

Order state, entitlement and payment ledger are written in one transaction. Replays
cannot extend a subscription twice. Late paid events can fulfill expired orders.
Refund/chargeback revokes only this order's internal ledger key; replayed paid events
cannot resurrect it. Reversals are audited. This does not initiate a monetary refund.
Existing subscriptions and manual keys remain intact. Old Pally sources are dormant.

Checks: `cargo test pay::`, `cargo test recode::`; `rollypay_smoke.py` runs against an
isolated DB with a dedicated binary, using `ROLLYPAY_TEST_BINARY` and
`ROLLYPAY_TEST_DIR`. Tests must never point at production data. The smoke test covers
auth, origin, consent, ownership, tampering, sandbox isolation, duplicate/late events,
refunds and redirects. Provider sandbox creation/simulation needs the real API key
but must always send `test:true`; never add sandbox access to customer accounts.
