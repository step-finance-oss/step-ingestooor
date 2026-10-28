> **Historical reference snapshot**
>
> This source is preserved for study and reuse. It is not maintained or verified
> to build or run with current dependencies. Historical instructions below may
> rely on unavailable services, repositories or infrastructure. CI and GitHub
> Actions configuration have been removed, together with repository synchronization
> scripts and private contact/tracking references where present.
>
> Original Step Finance contributions are licensed under [MIT](LICENSE).
> Third-party portions retain their applicable upstream terms; see
> [third-party notices](THIRD_PARTY_NOTICES.md). The snapshot as a whole is
> not represented as exclusively MIT-licensed.

# Step Ingestooor

A high-performance Solana blockchain data ingestion system that processes transactions and generates structured data (Dooots) for analytics, dashboards, and real-time applications.

## 🚀 Quick Start

### Prerequisites

- [Rust](https://www.rust-lang.org/learn/get-started) (latest stable)
- [rust-analyzer](https://rust-analyzer.github.io/) (recommended for development)

### Environment Setup

1. **Copy and configure environment variables:**

   ```bash
   # Create .env file from example
   cp .env.example .env

   # Edit .env with your configuration
   code .env
   ```

2. **Required environment variables:**

   ```bash
   # RabbitMQ Connection String
   RABBITMQ_URL=amqp://guest:guest@localhost:5672

   # Solana RPC Endpoint
   RPC_URL=https://api.mainnet-beta.solana.com

   # RPC Token (if required by your RPC provider)
   RPC_TOKEN=your_rpc_token_here

   # Debug Username for development
   DEBUG_USERNAME=your_debug_username_here
   ```

3. **Run the ingestooor:**
   ```bash
   # Build and run in release mode for optimal performance
   cargo run --release
   ```

## 🏗️ Architecture Overview

### Core Components

- **Engine** (`crates/engine/`): Main binary that orchestrates the ingestion pipeline
- **Parsers** (`crates/parsers/`): Transaction parsers that extract data and generate Dooots
- **IDL Generators** (`crates/idl-gens/`): Rust types generated from Solana program IDLs
- **State Initializer** (`crates/state_initializer/`): ClickHouse data writer and backfill utilities
- **Common Libraries** (`libs/step-common-rust/`): Shared utilities and trait definitions

### Data Flow

```
Solana Transactions → RabbitMQ → Parsers → Dooots → ClickHouse/PostgreSQL
```

## 📊 Understanding Dooots

### What are Dooots?

Dooots are structured data representations extracted from Solana transactions. They represent the core data model for analytics, dashboards, and real-time applications.

**Historical Note:** Dooots were previously named "Schemas" but were renamed as the product matured.

### Dooot Types

Dooots are categorized into three logical types:

#### 🗺️ Global State Dooots

Global state data that is not user-specific. Used for:

- Protocol-level analytics (LP rates, lending rates over time)
- Mint metadata and underlying token information
- Global market data

**Examples:**

- [`MintUnderlyingsGlobal`](libs/step-common-rust/crates/ingestooor-sdk/src/dooot/mint_underlyings.rs) - Token underlying amounts for LP positions
- [`MintInfo`](libs/step-common-rust/crates/ingestooor-sdk/src/dooot/mint_metadata.rs) - Token metadata and information
- [`OraclePriceEvent`](libs/step-common-rust/crates/ingestooor-sdk/src/dooot/oracle_price.rs) - Price oracle updates

#### 👤 User State Dooots

User-specific state data representing wallet positions and balances:

- Token balances and positions
- Protocol-specific user states
- Dashboard API data for position calculations

**Examples:**

- [`TokenBalanceUser`](libs/step-common-rust/crates/ingestooor-sdk/src/dooot/spl.rs) - User token balances
- [`SolBalanceUser`](libs/step-common-rust/crates/ingestooor-sdk/src/dooot/sol_balance.rs) - User SOL balances
- [`ClmmUser`](libs/step-common-rust/crates/ingestooor-sdk/src/dooot/clmm.rs) - User CLMM positions

#### ⚡ Event Dooots

Event data representing actions that occurred:

- Token transfers and swaps
- Trading activity
- Protocol interactions

**Examples:**

- [`TokenTransferEvent`](libs/step-common-rust/crates/ingestooor-sdk/src/dooot/spl.rs) - Token transfer events
- [`SwapEvent`](libs/step-common-rust/crates/ingestooor-sdk/src/dooot/swap.rs) - DEX swap events
- [`ClobFillEvent`](libs/step-common-rust/crates/ingestooor-sdk/src/dooot/clob.rs) - CLOB fill events

## 🔧 Parser System

### How Parsers Work

Parsers implement the `TransactionParserTrait` and process transactions to generate Dooots:

```rust
#[async_trait]
impl TransactionParserTrait for TokenTransfer {
    fn get_name(&self) -> &'static str {
        "Token transfer"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![
            TransactionParserFilter::ProgramId(TOKEN_PROGRAM_ID.to_string()),
            TransactionParserFilter::ProgramId(TOKEN_2022_PROGRAM_ID.to_string()),
        ]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        // Parse transaction and generate Dooots
        // ...
    }
}
```

### Available Parsers

#### DEX & AMM Parsers

- **Jupiter** (`crates/parsers/src/swaps/jupiter.rs`) - Jupiter aggregator swaps
- **Raydium** (`crates/parsers/src/amm/raydium_v4.rs`, `raydium_v5.rs`, `raydium_v6.rs`) - Raydium AMM operations
- **Orca** (`crates/parsers/src/amm/clmm/orca.rs`) - Orca CLMM operations
- **Meteora** (`crates/parsers/src/amm/meteora_amm.rs`) - Meteora AMM operations
- **OpenBook** (`crates/parsers/src/dex/openbook/parser.rs`) - OpenBook DEX operations

#### Lending & Yield Parsers

- **Kamino** (`crates/parsers/src/lending/kamino.rs`) - Kamino lending operations
- **Marginfi** (`crates/parsers/src/lending/marginfi.rs`) - Marginfi lending operations
- **Solend** (`crates/parsers/src/lending/solend/parser.rs`) - Solend lending operations

#### Staking & Farming Parsers

- **Marinade** (`crates/parsers/src/staking/marinade.rs`) - Marinade staking operations
- **Kamino Farms** (`crates/parsers/src/farms/kamino_farms.rs`) - Kamino farming operations
- **Raydium Farms** (`crates/parsers/src/farms/raydium_v3.rs`, `raydium_v5.rs`, `raydium_v6.rs`) - Raydium farming operations

#### SPL Parsers

- **Token Transfer** (`crates/parsers/src/spl/tokens.rs`) - SPL token operations
- **SOL Transfer** (`crates/parsers/src/system/sol_transfer.rs`) - Native SOL transfers
- **Native Staking** (`crates/parsers/src/system/native_staking.rs`) - Solana staking operations

## 🆕 Creating New Dooots

### 1. Define the Dooot Structure

Create a new struct in the appropriate module under `libs/step-common-rust/crates/ingestooor-sdk/src/dooot/`:

```rust
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use step_ingestooor_clickhouse_derive::ClickhouseDooot;
use step_ingestooor_derive::Dooot;
use ts_rs::TS;

#[derive(
    Serialize, Deserialize, Debug, Clone, Default, Dooot, TS, ClickhouseDooot, clickhouse::Row,
)]
#[dooot(name = "MyNewDooot", topic)]
#[clickhouse(
    table = "incoming_my_new_dooot",
    queue = "my_new_dooot",
    flush_ms = 5000
)]
pub struct MyNewDooot {
    pub time: NaiveDateTime,
    #[topic]
    pub user_pubkey: String,
    pub amount: u64,
    pub token_mint: String,
}
```

### 2. Add to the Dooot Enum

Add your new Dooot to the main enum in `libs/step-common-rust/crates/ingestooor-sdk/src/dooot/mod.rs`:

```rust
pub enum Dooot {
    // ... existing variants ...
    MyNewDooot(MyNewDooot),
}
```

### 3. Create a Parser

Implement a parser that generates your Dooot:

```rust
use crate::{TransactionParserFilter, TransactionParserTrait};
use step_ingestooor_sdk::dooot::{Dooot, MyNewDooot};

pub struct MyNewParser;

#[async_trait]
impl TransactionParserTrait for MyNewParser {
    fn get_name(&self) -> &'static str {
        "My New Parser"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId("YourProgramId".to_string())]
    }

    async fn invoke(
        &self,
        txn: &impl TransactionLike,
        dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        // Parse transaction and generate MyNewDooot
        let new_dooot = MyNewDooot {
            time: txn.get_block_time(),
            user_pubkey: "user_pubkey".to_string(),
            amount: 1000,
            token_mint: "token_mint".to_string(),
        };

        dooot.push(Dooot::MyNewDooot(new_dooot));
        Ok(())
    }
}
```

### 4. Register the Parser

Add your parser to the `TransactionParser` enum in `crates/parsers/src/lib.rs`:

```rust
pub enum TransactionParser {
    // ... existing variants ...
    MyNewParser,
}
```

### 5. Add to Parser Manager

Register your parser in `crates/parsers/src/lib.rs`:

```rust
pub fn construct_all_parsers() -> Vec<TransactionParser> {
    vec![
        // ... existing parsers ...
        TransactionParser::MyNewParser,
    ]
}
```

### 6. Add to Engine Tasks

Add a task for your new Dooot in `crates/engine/src/ingestooor.rs` around line 538-550:

```rust
let my_new_dooot_task = ch_data_writer.make_inserter_task::<MyNewDooot>()?;
```

### 7. Add to Select Statement

Add your task to the select statement in the same file around line 580-680:

```rust
tokio::select! {
    // ... existing variants ...
    _ = my_new_dooot_task => {
        log::info!("My new dooot task finished");
    }
}
```

### 8. Add to Tasks Collection

Add your task to the `tasks` collection in the same file around line 650-680:

```rust
let mut tasks = FuturesUnordered::new();
tasks.push(native_stake_event_task);
tasks.push(sol_transfer_task);
// ... existing tasks ...
tasks.push(my_new_dooot_task); // Add your new task here
```

### 9. Register in ClickHouse Writer

Add your Dooot to the ClickHouse writer macro in `crates/state_initializer/src/ch_writer_lite.rs`:

```rust
define_ch_writer_lite! {
    // ... existing dooots ...
    (MyNewDooot, MyNewDooot, i_my_new_dooot),
}
```

## 🧪 Testing

### Run Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

### Test Individual Parsers

```bash
# Test specific parser
cargo test -p step-ingestooor-parsers --test test_name

# Test with specific features
cargo test --features "test-feature"
```

## 🛠️ Development Tools

### Code Quality

```bash
# Linting
cargo clippy

# Auto-fix linting errors
cargo fix

# Format code
cargo fmt
```

### Performance Profiling

```bash
# Build in release mode for performance testing
cargo build --release

# Run with performance profiling
cargo run --release --profile
```

## 📚 Additional Resources

### Related Projects

- [step-common-rust](libs/step-common-rust/) - Shared utilities and common code
- [ingestooor-sdk](libs/step-common-rust/crates/ingestooor-sdk/) - Core SDK for Dooots and parsers
- [ingestooor-derive](libs/step-common-rust/crates/ingestooor-derive/) - Derive macros for Dooots

### Architecture Decisions

- **Atomicity**: All parser results are ignored if any parser fails, ensuring data consistency
- **Performance**: Hot functions are defined in `step-common-rust` for optimal performance
- **Extensibility**: New parsers and Dooots can be added without modifying core infrastructure

## 🚧 Future Development

### Pricing Engine

The Pricing Engine component has been built and is available at [veritas](https://github.com/step-finance/veritas/). This service:

- Reads Dooots from RabbitMQ in real-time
- Adjusts in-memory prices dynamically
- Outputs price messages for real-time pricing

The veritas project provides the real-time pricing functionality that was previously planned for this component.

## 🤝 Contributing

1. Follow the existing code patterns and naming conventions
2. Ensure all tests pass with `cargo test`
3. Run `cargo clippy` and `cargo fmt` before submitting
4. Add comprehensive tests for new parsers and Dooots
5. Update documentation for any new features

## 📄 License

Original Step Finance contributions are licensed under MIT; third-party portions retain their upstream terms. See [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
