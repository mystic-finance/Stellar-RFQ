# Redemption vault (Stellar)

`contracts/redemption_vault` batches redemptions of one RWA with its issuer
and pays them out in one stable. It is the Stellar counterpart of the EVM
`OctarineRedemptionVault`. The cross-chain REDEEM_CT and REDEEM_PUSH modes use
it, and anyone can redeem through it directly.

**Why claim tokens.** A redemption can take weeks. Each batch issues its own
claim token (CT), a standard SEP-41 token, so a holder can sell the claim
instead of waiting. The vault is also a price source for its CTs.

## Contracts

| Contract | Role |
|---|---|
| `redemption_vault` | One per RWA. Holds open batches and settled proceeds, and runs the batch lifecycle. |
| `claim_token` | One per batch, deployed by the vault. SEP-41, with the RWA's decimals; named `Octarine Claim <RWA> #<id>`, symbol `ct<RWA>-<id>`. Only its vault mints and burns (`burn_claim`). With a gate set, every holder except the vault must pass it. |
| `redemption_account` | A pool of issuer-facing addresses, deployed by the vault at predictable addresses (`account_address(i)`), so they can be whitelisted with the issuer in advance. Each holds one AUTO batch at a time, then is reused. Only the vault can call it. |
| `call_verifier` | The allowlist of issuer calls a vault may make from its accounts (AUTO mode). One verifier can serve many vaults. |

## Batch lifecycle

```
request_redeem ──▶ OPEN ──(AUTO: redeem_batch / MANUAL: withdraw_batch)──▶ SENT ──(settle / settle_manual)──▶ SETTLED ──▶ claim
     │                                                                        │
     └── cancel (burns CT, returns RWA)                                       └── reopen (AUTO: all RWA back → OPEN)
```

- **Request.** `request_redeem(owner, units, controller)` moves the owner's RWA
  into the open batch and mints that batch's CT one for one to `controller`.
  When the open batch leaves, the next request opens a new batch with a new CT.
- **Cancel.** `cancel(owner, id, amount, receiver)` burns CT and returns the RWA,
  only while the batch is open.
- **Claim.** `claim(owner, id, shares, receiver)` burns CT of a settled batch
  for its share of what the batch has left, so the last holder takes the
  rounding dust. If the issuer handed RWA back, holders also receive their
  share of that RWA.

## Two modes

`Settings.mode` is either AUTO or MANUAL. It can change only through the
timelock, and only when no batch is with the issuer. Each mode has its own
functions, and a batch can only settle the way it was sent.

### AUTO: the settler, through verified calls

1. **Send.** `redeem_batch(id, spender, calls)`:
   - moves the batch's RWA to a free redemption account;
   - runs `calls` from that account;
   - requires the account's RWA to fall by exactly the batch, or the call reverts.

   If `spender` is set, it is allowed to pull the RWA from the account for
   that one ledger.
2. **Settle.** `settle(id, calls, min_proceeds)` runs the calls that collect the
   proceeds, then moves the account's stable into the vault. The proceeds are
   whatever stable actually arrived, and must be at least `min_proceeds`. The
   settler only sets that minimum and cannot overstate the amount.
3. **Reopen.** `reopen(id, calls, returned)` records RWA the issuer handed back.
   - **Part of the batch back:** `redeem_batch` on a SENT batch sends it again.
   - **All of it back:** the batch is OPEN again.
   - **Still unsent at settlement:** that RWA goes to holders in kind.

Every call runs from the account, never from the vault, and must pass the
verifier. Calls to the stable, the account itself, the vault, or any of the
vault's CTs are refused.

A verifier rule names a target and a function, and fixes each argument to a
template value. Two kinds of argument are exceptions, each marked by a bit:
- **free:** not checked at all (typically the amount);
- **account:** must be the account the call runs from (on EVM, the template's `ACCOUNT` placeholder).

The verifier admin adds or re-enables rules, which take effect only after a
fixed delay set at deployment. During that delay the guardian or the admin can
disable or remove the rule, with immediate effect.

### MANUAL: the redemption operator, off-chain

1. **Withdraw.** `withdraw_batch(id, to)` sends an open batch's RWA to `to`,
   which must be on the timelocked `destinations` list. The operator then
   redeems with the issuer off-chain.
2. **Settle.** `settle_manual(id, proceeds, returned)` pulls `proceeds` in the
   stable, plus any RWA the issuer `returned`, from the operator. The proceeds
   must reach the **NAV floor**, rounded up in holders' favour:

   ```
   (units − returned) × price × (1 − floor_bps)
   ```

   - **Struck batch:** `price` is the strike.
   - **Otherwise:** the oracle's NAV now. With no fresh reading,
     settlement waits.

In MANUAL mode the verified-call functions refuse to run. In AUTO mode the
operator cannot move RWA at all.

## Two kinds of asset: strike at send, or price at settlement

`Settings.strike_at_send` covers both:

- **Struck (`true`):** the issuer fixes the cash at request. When a batch is
  sent, it is struck at the NAV then, and needs a fresh price to be sent.
- **Unstruck (`false`):** the batch is worth the NAV at settlement.

Changing it needs the timelock and no batch with the issuer.

`claim_value(id, price)` is what one CT is worth in the stable, scaled by 1e18,
net of the redemption fee:

| Batch | Value per CT |
|---|---|
| Settled | its share of the cash, plus its share of any returned RWA at `price` |
| Struck, not settled | the strike |
| Otherwise | `price` |

`get_price(ct, stable)` gives the same value at the live NAV, in the shared
oracle interface. The RFQ can then price a CT by registering the vault as that
CT's oracle with `set_oracle(ct, stable, …)`.

## Fees

`fee_bps` (at most 20%) of each batch's proceeds goes to `fee_recipient` at
settlement. Holders share the rest.

## Roles and governance

| Role | Can |
|---|---|
| admin | submit changes, add redemption accounts |
| guardian | revoke any pending change, pause requests |
| settler | run AUTO batches: `redeem_batch`, `settle`, `reopen` |
| operator | run MANUAL batches: `withdraw_batch`, `settle_manual` |

**Timelocked changes** (`submit`, then `accept(kind)` by anyone once the delay
has passed, or `revoke` before then):

| Kind | Change |
|---|---|
| 0 | `Settings`: mode, strike at send, verifier, gate, oracle and max age, fee, floor, destinations |
| 1 | `Roles` |
| 2 | `Skim(token, to)`: anything the vault holds beyond what it owes |
| 3 | `Delay` |

The mode switch, the operator and the destinations are what would let the RWA
leave the vault, and all of them sit behind the delay. Deploy with a delay of 0
to configure the vault, then queue the real delay.

**Pause** stops new requests only. Cancels, claims and settlement keep working.

A gate change applies to the open batch's CT and every later one. Batches
already sent keep the gate they had.

**A gate must admit `crosschain_settle`.** Cross-chain fills mint CTs to it
first, then pass them on to the recipient, or owe them to the recipient for
`withdraw` if the gate refuses it.

## Differences from the EVM vault

- **MANUAL mode** (operator withdrawal and the NAV floor) exists only on Stellar
  for now. The EVM vault will get it after the current EVM work lands.
- **No auto-paying subscribers.** Holders call `claim`.
- **No ERC-7540 operators.** Soroban authorisation covers delegation.
