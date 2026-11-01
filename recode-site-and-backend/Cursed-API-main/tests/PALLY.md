# Pally integration

Protocol checked against https://pally.info/reference/api on 2026-10-26.

Merchant settings:

- Shop: https://cursedclient.xyz
- Success: https://cursedclient.xyz/api/pay/ok
- Fail: https://cursedclient.xyz/api/pay/fail
- Result: https://cursedclient.xyz/api/pay/pally
- Refund / Chargeback: leave empty; automated revocation is not implemented.

Set `PALLY_SHOP_ID` and `PALLY_API_TOKEN` in `/etc/cursed/pally.env`
(root:cursed, 0640). Install `deploy/pally.conf` as
`/etc/systemd/system/cursed-api.service.d/pally.conf`, then reload systemd
and restart only after the tested application has been deployed. Credentials
are never sent to the browser. API is the documented `https://pal24.pro/api/v1`.
If the token has an IP allowlist, allow the server's outbound IP.

Prices are server-owned: 30 days 299 RUB, 90 days 499 RUB, lifetime 699 RUB.
Commission is paid by the merchant; the checkout amount matches the displayed price.
Only authenticated users can create an order; consent and a supported term/method
are required. Ten checkout attempts per account per hour. Existing lifetime access
cannot be bought again. Existing key activation remains available.

Notifications require the documented MD5 signature, order/bill/currency binding,
and an authenticated bill status lookup. Pally's callback signature does not cover
its Status field, so callback status alone never grants access. A transaction writes
the payment, fulfilled marker, and non-redeemable subscription ledger entry together.
Repeat notifications do not extend access again. Missing accounts are not credited.
Neither success nor failure browser redirects constitute payment evidence.

The return page polls the authenticated status endpoint for up to two minutes;
this also recovers payments whose notification was missed. If the user closes the
page and all Pally notification retries fail, manual reconciliation is still needed.
Refunds/chargebacks must be reviewed manually; do not configure their callbacks yet.

Validation: `cargo test --locked pay::` on Linux; HTTP smoke tests with an isolated
database and dummy credentials; `node --check` for the changed JavaScript.
A real end-to-end payment requires an approved merchant, valid credentials and
user-authorized payment. Until then do not describe real payments as verified.
