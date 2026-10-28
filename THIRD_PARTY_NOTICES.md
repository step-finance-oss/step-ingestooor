# Third-party notices

The MIT license in this snapshot applies to original Step Finance contributions only.
Code adapted from or attributed to upstream projects remains subject to its applicable
upstream license and copyright notices. Existing source comments and attribution are retained.
This document does not relicense upstream material or assert that all included licenses are compatible.
References below identify attributed portions; they are not a complete legal provenance audit.

## Preserved upstream license texts

| Upstream project | Referenced local portion | Upstream license text |
| --- | --- | --- |
| [metaplex-foundation/mpl-token-metadata](https://github.com/metaplex-foundation/mpl-token-metadata/blob/87af91762ff056a1dc7ccba84be06cea2491510b/LICENSE) | `crates/parsers/src/mint_metadata/metaplex/` | [Upstream-specific terms](third-party-licenses/metaplex-foundation__mpl-token-metadata.txt) |
| [Ellipsis-Labs/phoenix-v1](https://github.com/Ellipsis-Labs/phoenix-v1/blob/master/LICENSE) | `crates/parsers/src/dex/phoenix/` | [MIT](third-party-licenses/Ellipsis-Labs__phoenix-v1.txt) |
| [solendprotocol/solana-program-library](https://github.com/solendprotocol/solana-program-library/blob/mainnet/LICENSE) | `crates/parsers/src/lending/solend/` | [Upstream-specific terms](third-party-licenses/solendprotocol__solana-program-library.txt) |
| [CKS-Systems/manifest](https://github.com/Bonasa-Tech/manifest/blob/918d0aa3da6df95f300be987986b3f4542138417/LICENSE) | `crates/parsers/src/dex/manifest.rs` | [GPL-3.0](third-party-licenses/CKS-Systems__manifest.txt) |
| [mrgnlabs/marginfi-v2](https://github.com/0dotxyz/marginfi-v2/blob/main/LICENSE) | `crates/idl-gens/src/marginfi_gen.rs` | [Apache-2.0](third-party-licenses/mrgnlabs__marginfi-v2.txt) |
| [raydium-io/raydium-sdk](https://github.com/raydium-io/raydium-sdk-v1/blob/master/LICENSE) | `crates/parsers/src/farms/raydium_v3.rs; crates/parsers/src/farms/raydium_v5.rs` | [GPL-3.0](third-party-licenses/raydium-io__raydium-sdk.txt) |
| [coral-xyz/anchor](https://github.com/otter-sec/anchor/blob/v0.29.0/LICENSE) | `libs/step-common-rust/crates/ingestooor-sdk/src/trait_defs/transaction.rs` | [Apache-2.0](third-party-licenses/coral-xyz__anchor.txt) |
| [anza-xyz/agave](https://github.com/anza-xyz/agave/blob/master/LICENSE) | `libs/step-common-rust/crates/ingestooor-sdk/src/utils/spl_ixs.rs` | [Apache-2.0](third-party-licenses/anza-xyz__agave.txt) |
| [solana-program/token-2022](https://github.com/solana-program/token-2022/blob/main/LICENSE) | `libs/step-common-rust/crates/ingestooor-sdk/src/utils/token_metadata.rs; libs/step-common-rust/crates/ingestooor-sdk/src/utils/t22_extensions.rs` | [Apache-2.0](third-party-licenses/solana-program__token-2022.txt) |
| [solana-program/token-metadata](https://github.com/solana-program/token-metadata/blob/main/LICENSE) | `libs/step-common-rust/crates/ingestooor-sdk/src/utils/token_metadata.rs` | [Apache-2.0](third-party-licenses/solana-program__token-metadata.txt) |

## Additional source references

The original source links below are retained. An upstream license text was not
retrieved for these portions during this preparation; no MIT grant is made for them.
Their applicable terms must be established before redistribution.

- **DeFi-Carrot/jup-carrot-integration**: `crates/parsers/src/vaults/carrot/`. https://github.com/DeFi-Carrot/jup-carrot-integration
- **Ellipsis-Labs/phoenix-sdk**: `crates/parsers/src/dex/phoenix/parser.rs`. https://github.com/Ellipsis-Labs/phoenix-sdk/blob/afb23a9db6c745159e12781bea363044c8831cb7/rust/phoenix-sdk-core/src/sdk_client_core.rs
- **mercurial-finance/mercurial-dynamic-amm-sdk**: `crates/parsers/src/vaults/meteora_vaults.rs`. https://github.com/mercurial-finance/mercurial-dynamic-amm-sdk
- **Other protocol and interface references**: `Other parser, IDL and SDK files`. Source comments retain the original references.

Dependencies fetched by Cargo are governed by their own licenses and are not relicensed by this snapshot.
