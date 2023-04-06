# CW20 Merkle Airdrop

This is a [cw-tokens/cw20-merkle-airdrop-contract](https://github.com/CosmWasm/cw-tokens/tree/main/contracts/cw20-merkle-airdrop) with several patches:
- removed native token distribution functionality;
- removed `burn`, `burn_all` and `withdraw` ExecuteMsg's;
- `claim` patched to issue `AddVesting` message to credits contract;
- `withdraw_all` patched to burn cNTRN tokens and send (received in exchange for burning) NTRN tokens to reserve contract. `withdraw_all` can only be called after 3 months after the end of the event;
- stages logic removed, since we will only need one airdrop stage, also merged register merkle tree and instantiate messages into one;
- unified owner logic, similar to other TGE contracts;
- tests moved to separate file;
- enforced start/expiration logic to accept only timestamps, since blocks will break vesting logic.

## The NTRN Airdrop
70 million NTRN tokens will be allocated to ATOM stakers at genesis. These tokens represent 7% of the total supply and 58.3% of the initial circulating supply, and will be allocated in two batches:
- 40,000,000 NTRN tokens, or 4% of the total supply, will be allocated to accounts with over 1 ATOM staked on Block #12900000 (2022–11–19)
- 30,000,000 NTRN tokens, or 3% of the total supply, will be allocated to accounts that voted on Prop 72, regardless of their vote (Yes, No, Abstain, or NoWithVeto) and whether they voted directly or through their validator.

The NTRN airdrop is subject to certain exclusions:

- U.S. persons, sanctioned persons and residents of sanctioned countries are not eligible to participate.
- Validators affiliated with centralized exchanges and custodians, as well as their delegators, are also excluded.

Furthermore, to ensure that tokens are not allocated to “dust” accounts nor concentrated within the few wealthiest accounts, the following boundaries are in place:

- Minimum stake: 1 ATOM. This minimum boundary excludes ~38% of all Cosmos Hub accounts, the vast majority of which are “dust” wallets.
- Maximum stake: 1,000,000 ATOM. This maximum threshold only concerns the ~25 wealthiest accounts, yet it reduces the eligible stake by ~33%, thereby increasing the allocation to all other participants by a third.

Introducing a “whale cap” makes the airdrop vulnerable to Sybil accounts: imagine an entity with ten million ATOM distributed across ten wallets or more. Since none of the wallets hit the maximum threshold, they would receive ten times more tokens than intended.

To alleviate this concern, Neutron will be organizing an open Sybil hunt. Identified Sybil clusters with aggregated staked ATOM balances above one million will be dealt with according to the following scenarios, whichever comes first:

- Self-report: Honest entities who owned more than one million staked ATOM across multiple wallets may self report to retain their right to the maximum airdrop allocation. Tokens in excess of the max reward will be redistributed to the community.
- Community Report: Any third party may report a Sybil cluster by providing on-chain evidence that a single entity owns more than a million staked ATOM across multiple accounts. If the evidence is found to be conclusive, the author of the report will earn 10% of the max airdrop allocation, and the sybil cluster's addresses will be entirely removed from the airdrop.

## Reporting a group of Sybil attacker addresses

To report a group of Sybil attacker addresses, create an issue on this repo and use the "Sybil Report" template.

Rules:

- Reports will be reviewed on a first-come, first serve basis.
- Reports must contain addresses which, together, would receive more than the max airdrop allocation `TODO:INSERT VALUE HERE` according to the list of eligible addresses which can be found here `TODO:INSERT PATH TO DOC`
- The methodology must be well explained and easy to understand. Methodology that has a non-negligible chance of eliminating non-sybil accounts will not be considered
- Self-reports should provide a list of transactions or signatures with “NTRN Self-Report” as a memo from each of the self-reported addresses. If discovered, fraudulent partial self-reporting (e.g. only disclosing some of the controlled addresses to game the airdrop “a little bit”) will lead to the removal of the entire allocation.

Happy hunting 🏹
