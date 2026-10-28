# Step Ingestooor State Initializer

This allows parsers to also have logic for initializing their state from a ledger snapshot.

## Setup and running the state initializer

0. Obtain a Solana ledger snapshot. You can grab latest from [here](https://api.mainnet-beta.solana.com/snapshot.tar.bz2)

1. Clone the repository

   ```sh
   git clone git@github.com:step-finance/step-ingestooor.git
   ```

2. Implement `StateSource` for the parser you wish to backfill for.

3. See which options you need for your backfill

   ```sh
   cargo run --bin state_initializer -- -h
   ```

   Note how you must pass an aditional `--` before the binary's flags. Otherwise any flags passed without this, are passed to `cargo`.

4. Ensure your environment variables are set for the correct environment you wish to backfill to
5. Run your backfill

   Example:

   ```sh
   cargo run --bin state_initializer -- -p TeamRacoonsReferrals --data "Any extra data you'd like to pass in"
   ```
