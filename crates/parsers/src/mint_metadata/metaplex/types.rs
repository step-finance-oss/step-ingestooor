//! Ripped from https://github.com/metaplex-foundation/mpl-token-metadata/blob/87af91762ff056a1dc7ccba84be06cea2491510b/programs/token-metadata/program/src/instruction/mod.rs#L49

#![allow(deprecated)]

use borsh::BorshDeserialize;
use mpl_token_metadata::{instructions::*, types::*};

/// Instructions supported by the Metadata program.
#[repr(C)]
#[derive(Debug, BorshDeserialize)]
pub enum MetadataInstruction {
    /// Create Metadata object.
    /// 0
    #[deprecated(note = "Metaplex deprecated this instruction")]
    CreateMetadataAccount,

    /// Update a Metadata
    /// 1
    #[deprecated(note = "Metaplex deprecated this instruction")]
    UpdateMetadataAccount,

    /// Register a Metadata as a Master Edition V1, which means Editions can be minted.
    /// Henceforth, no further tokens will be mintable from this primary mint. Will throw an error if more than one
    /// token exists, and will throw an error if less than one token exists in this primary mint.
    /// 2
    #[deprecated(note = "Metaplex deprecated this instruction")]
    DeprecatedCreateMasterEdition,

    /// Given an authority token minted by the Printing mint of a master edition, and a brand new non-metadata-ed mint with one token
    /// make a new Metadata + Edition that is a child of the master edition denoted by this authority token.
    /// 3
    #[deprecated(note = "Metaplex deprecated this instruction")]
    DeprecatedMintNewEditionFromMasterEditionViaPrintingToken,

    /// Allows updating the primary sale boolean on Metadata solely through owning an account
    /// containing a token from the metadata's mint and being a signer on this transaction.
    /// A sort of limited authority for limited update capability that is required for things like
    /// Metaplex to work without needing full authority passing.
    /// 4
    UpdatePrimarySaleHappenedViaToken,

    /// Reserve up to 200 editions in sequence for up to 200 addresses in an existing reservation PDA, which can then be used later by
    /// redeemers who have printing tokens as a reservation to get a specific edition number
    /// as opposed to whatever one is currently listed on the master edition. Used by Auction Manager
    /// to guarantee printing order on bid redemption. AM will call whenever the first person redeems a
    /// printing bid to reserve the whole block
    /// of winners in order and then each winner when they get their token submits their mint and account
    /// with the pda that was created by that first bidder - the token metadata can then cross reference
    /// these people with the list and see that bidder A gets edition #2, so on and so forth.
    ///
    /// NOTE: If you have more than 20 addresses in a reservation list, this may be called multiple times to build up the list,
    /// otherwise, it simply wont fit in one transaction. Only provide a total_reservation argument on the first call, which will
    /// allocate the edition space, and in follow up calls this will specifically be unnecessary (and indeed will error.)
    /// 5
    #[deprecated(note = "Metaplex deprecated this instruction")]
    DeprecatedSetReservationList,

    /// Create an empty reservation list for a resource who can come back later as a signer and fill the reservation list
    /// with reservations to ensure that people who come to get editions get the number they expect. See SetReservationList for more.
    /// 6
    #[deprecated(note = "Metaplex deprecated this instruction")]
    DeprecatedCreateReservationList,

    /// Sign a piece of metadata that has you as an unverified creator so that it is now verified.
    /// 7
    SignMetadata,

    /// Using a one time authorization token from a master edition v1, print any number of printing tokens from the printing_mint
    /// one time, burning the one time authorization token.
    /// 8
    #[deprecated(note = "Metaplex deprecated this instruction")]
    DeprecatedMintPrintingTokensViaToken,

    /// Using your update authority, mint printing tokens for your master edition.
    /// 9
    #[deprecated(note = "Metaplex deprecated this instruction")]
    DeprecatedMintPrintingTokens,

    /// Register a Metadata as a Master Edition V2, which means Edition V2s can be minted.
    /// Henceforth, no further tokens will be mintable from this primary mint. Will throw an error if more than one
    /// token exists, and will throw an error if less than one token exists in this primary mint.
    /// 10
    #[deprecated(note = "Metaplex deprecated this instruction")]
    CreateMasterEdition,

    /// Given a token account containing the master edition token to prove authority, and a brand new non-metadata-ed mint with one token
    /// make a new Metadata + Edition that is a child of the master edition denoted by this authority token.
    /// 11
    MintNewEditionFromMasterEditionViaToken(MintNewEditionFromMasterEditionViaTokenArgs),

    /// Converts the Master Edition V1 to a Master Edition V2, draining lamports from the two printing mints
    /// to the owner of the token account holding the master edition token. Permissionless.
    /// Can only be called if there are currenly no printing tokens or one time authorization tokens in circulation.
    /// 12
    ConvertMasterEditionV1ToV2,

    /// Proxy Call to Mint Edition using a Store Token Account as a Vault Authority.
    /// 13
    #[deprecated(note = "Metaplex deprecated this instruction")]
    MintNewEditionFromMasterEditionViaVaultProxy(MintNewEditionFromMasterEditionViaTokenArgs),

    /// Puff a Metadata - make all of it's variable length fields (name/uri/symbol) a fixed length using a null character
    /// so that it can be found using offset searches by the RPC to make client lookups cheaper.
    /// 14
    PuffMetadata,

    /// Update a Metadata with is_mutable as a parameter
    /// 15
    UpdateMetadataAccountV2(UpdateMetadataAccountV2InstructionArgs),

    /// Create Metadata object.
    /// 16
    #[deprecated(note = "Metaplex deprecated this instruction")]
    CreateMetadataAccountV2,

    /// Register a Metadata as a Master Edition V2, which means Edition V2s can be minted.
    /// Henceforth, no further tokens will be mintable from this primary mint. Will throw an error if more than one
    /// token exists, and will throw an error if less than one token exists in this primary mint.
    /// 17
    CreateMasterEditionV3(CreateMasterEditionV3InstructionArgs),

    /// If a MetadataAccount Has a Collection allow the UpdateAuthority of the Collection to Verify the NFT Belongs in the Collection.
    /// 18
    VerifyCollection,

    /// Utilize or Use an NFT , burns the NFT and returns the lamports to the update authority if the use method is burn and its out of uses.
    /// Use Authority can be the Holder of the NFT, or a Delegated Use Authority.
    /// 19
    // Rent is technically not needed but there isn't a way to "ignore" an account without
    // preventing latter accounts from being passed in.
    Utilize(UtilizeInstructionArgs),

    /// Approve another account to call [utilize] on this NFT.
    /// 20
    ApproveUseAuthority(ApproveUseAuthorityInstructionArgs),

    /// Revoke account to call [utilize] on this NFT.
    /// 21
    RevokeUseAuthority,

    /// If a MetadataAccount Has a Collection allow an Authority of the Collection to unverify an NFT in a Collection.
    /// 22
    UnverifyCollection,

    /// Approve another account to verify NFTs belonging to a collection, [verify_collection] on the collection NFT.
    /// 23
    ApproveCollectionAuthority,

    /// Revoke account to call [verify_collection] on this NFT.
    /// 24
    RevokeCollectionAuthority,

    /// Allows the same Update Authority (Or Delegated Authority) on an NFT and Collection to perform [update_metadata_accounts_v2]
    /// with collection and [verify_collection] on the NFT/Collection in one instruction.
    /// 25
    SetAndVerifyCollection,

    /// Allow freezing of an NFT if this user is the delegate of the NFT.
    /// 26
    FreezeDelegatedAccount,

    /// Allow thawing of an NFT if this user is the delegate of the NFT.
    /// 27
    ThawDelegatedAccount,

    /// Remove Creator Verificaton.
    /// 28
    RemoveCreatorVerification,

    /// Completely burn a NFT, including closing the metadata account.
    /// 29
    BurnNft,

    /// Verify Collection V2, new in v1.3--supports Collection Details.
    /// If a MetadataAccount Has a Collection allow the UpdateAuthority of the Collection to Verify the NFT Belongs in the Collection.
    /// 30
    VerifySizedCollectionItem,

    /// Unverify Collection V2, new in v1.3--supports Collection Details.
    /// If a MetadataAccount Has a Collection allow an Authority of the Collection to unverify an NFT in a Collection.
    /// 31
    UnverifySizedCollectionItem,

    // Set And Verify V2, new in v1.3--supports Collection Details.
    /// Allows the same Update Authority (Or Delegated Authority) on an NFT and Collection to perform [update_metadata_accounts_v2]
    /// with collection and [verify_collection] on the NFT/Collection in one instruction.
    /// 32
    SetAndVerifySizedCollectionItem,

    /// Create Metadata object.
    /// 33
    CreateMetadataAccountV3(CreateMetadataAccountV3InstructionArgs),

    /// Set size of an existing collection.
    /// 34
    SetCollectionSize(SetCollectionSizeArgs),

    /// Set the token standard of the asset.
    /// 35
    SetTokenStandard,

    /// Set size of an existing collection using CPI from the Bubblegum program.  This is how
    /// collection size is incremented and decremented for compressed NFTs.
    /// 36
    BubblegumSetCollectionSize(SetCollectionSizeArgs),

    /// Completely burn a print edition NFT.
    /// 37
    BurnEditionNft,

    /// Create an escrow account to hold tokens.
    /// 38
    CreateEscrowAccount,

    /// Close the escrow account.
    /// 39
    CloseEscrowAccount,

    /// Transfer the token out of Escrow.
    /// 40
    TransferOutOfEscrow(TransferOutOfEscrowInstructionArgs),

    //---- New API
    /// Burns an asset, closing associated accounts.
    ///
    /// Supports burning the following asset types:
    /// - ProgrammableNonFungible
    /// - NonFungible
    /// - NonFungigbleEdition
    /// - Fungible
    /// - FungibleAsset
    ///
    /// Parent accounts only required for burning print editions are the accounts for the master edition
    /// associated with the print edition.
    /// The Token Record account is required for burning a ProgrammableNonFungible asset.
    ///
    /// This handler closes the following accounts:
    ///
    /// For ProgrammableNonFungible assets:
    /// - Metadata, Edition, Token, TokenRecord
    ///
    /// For NonFungible assets:
    /// - Metadata, Edition, Token
    ///
    /// For NonFungibleEdition assets:
    /// - Metadata, Edition, Token, and the EditionMarker, if all prints for it are burned.
    ///
    /// For Fungible assets:
    /// - Only the token account, if all tokens are burned.
    ///     41
    Burn(BurnArgs),

    /// Creates the metadata and associated accounts for a new or existing mint account.
    ///
    /// This instruction will initialize a mint account if it does not exist and
    /// the mint key is a signer on the transaction.
    ///
    /// When creating a non-fungible assert, the `master_edition` needs to be specified.
    /// 42
    Create(CreateArgs),

    /// Mints tokens from a mint account into the specified token account.
    ///
    /// This instruction will also initialized the associated token account if it does not exist – in
    /// this case the `token_owner` will be required. When minting `*NonFungible` assets, the `authority`
    /// must be the update authority; in all other cases, it must be the mint authority from the mint
    /// account.
    /// 43
    Mint(MintArgs),

    /// Creates a delegate for an asset.
    ///
    /// A delegate has a role associated, which determines what actions the delegate can perform. There are
    /// two types of delegate:
    ///   1. Persistent delegate: only one delegate can exist at the same time for `Transfer`, `Sale` and
    ///      `Utility` actions (pda of ["metadata", program id, mint id, "persistent_delegate", token owner id])
    ///   2. Multiple delegates: for `Authority`, `Collection`, `Update` and `Uses` actions (pda of ["metadata",
    ///      program id, mint id, role, update authority id, delegate owner id])
    ///         44
    Delegate(DelegateArgs),

    /// Revokes a delegate.
    ///
    /// A delegate can revoke itself by signing the transaction as the 'approver'.
    ///     45
    Revoke(RevokeArgs),

    /// Locks an asset. For non-programmable assets, this will also freeze the token account.
    ///
    /// The configurable `authorization_rules` only apply to `ProgrammableNonFungible` assets and
    /// it may require additional accounts to validate the rules.
    /// 46
    Lock(LockArgs),

    /// Unlocks an asset. For non-programmable assets, this will also thaw the token account.
    ///
    /// The configurable `authorization_rules` only apply to `ProgrammableNonFungible` assets and
    /// it may require additional accounts to validate the rules.
    /// 47
    Unlock(UnlockArgs),

    /// Migrates an asset to a ProgrammableAsset type.
    /// 48
    #[deprecated(note = "Metaplex deprecated this instruction")]
    Migrate,

    /// Transfer an asset.
    ///
    /// The configurable `authorization_rules` only apply to `ProgrammableNonFungible` assets and
    /// it may require additional accounts to validate the rules.
    /// 49
    Transfer(TransferArgs),

    /// Updates the metadata of an asset.
    ///
    /// The configurable `authorization_rules` only apply to `ProgrammableNonFungible` assets and
    /// it may require additional accounts to validate the rules.
    /// 50
    Update(UpdateArgs),

    /// Uses an asset.
    ///
    /// Use Authority can be the owner of the asset or a delegated use authority.
    ///
    /// The configurable `authorization_rules` only apply to `ProgrammableNonFungible` assets and
    /// it may require additional accounts to validate the rules.
    /// 51
    Use(UseArgs),

    /// Verifies that an asset was created by a specific creator or belongs in an specified collection.
    ///
    /// Depending on the type of verification (e.g., creator or collection), additional accounts
    /// are required.
    /// 52
    Verify(VerificationArgs),

    /// Unverifies that an asset was created by a specific creator or belongs in an specified collection.
    ///
    /// Depending on the type of verification (e.g., creator or collection), additional accounts
    /// are required.
    /// 53
    Unverify(VerificationArgs),

    /// Collect fees stored on PDA accounts.
    /// 54
    Collect,

    /// Given a token account containing the master edition token to prove authority, and a brand new non-metadata-ed mint with one token
    /// make a new Metadata + Edition that is a child of the master edition denoted by this authority token.
    /// Account index 18, holder_delegate_record, is optional and only used for escrowless edition printing. It is fetched from the remaining accounts.
    /// 55
    Print(PrintArgs),
}
