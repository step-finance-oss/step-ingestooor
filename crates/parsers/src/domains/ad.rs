use anyhow::Result;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use solana_sdk::{pubkey, pubkey::Pubkey};
use step_ingestooor_sdk::{
    dooot::Dooot,
    trait_defs::{discriminatable::Discriminatable, transaction::TransactionLike},
    utils::step_utils::StepUtils,
};

use crate::{TransactionParserFilter, TransactionParserTrait};

pub const ANS_PROGRAM_ID: Pubkey = pubkey!("ALTNSZ46uaAUU7XUV6awvdorLGqAsPwa9shm7h4uP2FK");

pub const TLD_HOUSE_PROGRAM_ID: Pubkey = pubkey!("TLDHkysf5pCnKsVA4gXpNvmy7psXLPEu4LAdDJthT9S");
const MAIN_DOMAIN_DISCRIMINATOR: [u8; 8] = [109, 239, 227, 199, 98, 226, 66, 175];

pub const NAME_HOUSE_PROGRAM_ID: Pubkey = pubkey!("NH3uX6FtVE2fNREAioP7hm5RaozotZxeL6khU1EHx51");
// const NAME_RECORD_HEADER_DISCRIMINATOR: [u8; 8] = [68, 72, 88, 44, 15, 167, 103, 243];
// const NFT_RECORD_DISCRIMINATOR: [u8; 8] = [174, 190, 114, 100, 177, 14, 90, 254];

#[derive(Clone, Debug, BorshDeserialize, Eq, PartialEq, Default)]
pub struct MainDomain {
    /// name account of the main domain
    pub name_account: Pubkey,
    /// tld of the domain of interest
    pub tld: String,
    /// domain of the main domain
    pub domain: String,
}
impl Discriminatable for MainDomain {
    fn discriminator() -> (Pubkey, &'static [u8]) {
        (TLD_HOUSE_PROGRAM_ID, &MAIN_DOMAIN_DISCRIMINATOR)
    }
}

#[derive(Clone, Debug, BorshDeserialize, Eq, PartialEq, Default)]
pub struct NameRecordHeader {
    /// Names are hierarchical.  `parent_name` contains the account address of the parent
    /// name, or `Pubkey::default()` if no parent exists.
    pub parent_name: Pubkey,

    /// The owner of this name, could be nft_record if the domain has been wrapped.
    pub owner: Pubkey,

    /// The class of data this account represents
    /// If `Pubkey::default()` the data is unspecified.
    /// 'class' is a reserved word that cannot be used here.
    pub nclass: Pubkey,

    /// time is in unix timestamp
    /// programs must respect the expires_at.
    /// the data is invalid unless renewed by the owner or
    /// new owner comes with new data replacing the old one.
    /// defaults to 0
    pub expires_at: u64,
    pub created_at: u64,
    pub non_transferable: bool,
    /// is the name account data valid (not expired)
    pub is_valid: bool,
    // data sits here owner/parent name owner can add as many data as they please.
}

#[derive(Clone, Debug, BorshDeserialize, Eq, PartialEq, Default)]
pub enum Tag {
    Uninitialized,
    #[default]
    ActiveRecord,
    InactiveRecord,
}

#[derive(Clone, Debug, BorshDeserialize, Eq, PartialEq, Default)]
pub struct NftRecord {
    /// Tag
    pub tag: Tag,

    /// bump
    pub bump: u8,

    /// Name account of the record
    pub name_account: Pubkey,

    /// Record owner
    pub owner: Pubkey,

    /// NFT mint
    pub nft_mint_account: Pubkey,

    /// tld house
    pub tld_house: Pubkey,
}

//parser

pub struct AllDomainsParser;

#[async_trait]
impl TransactionParserTrait for AllDomainsParser {
    fn get_name(&self) -> &'static str {
        "AllDomains"
    }

    fn get_filters(&self) -> Vec<TransactionParserFilter> {
        vec![TransactionParserFilter::ProgramId(
            ANS_PROGRAM_ID.to_string(),
        )]
    }

    async fn invoke(
        &self,
        _decoded_transaction: &impl TransactionLike,
        _dooot: &mut Vec<Dooot>,
        _step_utils: &StepUtils,
    ) -> Result<()> {
        // let main_domains = decoded_transaction.get_accounts_typed_borsh::<MainDomain>()?;
        // for main_domain in main_domains.iter().filter(|a| a.is_changed) {
        // dooot.push(SomeNewDooot {
        //     time: decoded_transaction.get_block_time(),
        //     account_pubkey: main_domain.name_account.to_string(),
        //     domain: main_domain.domain.clone(),
        //     tld: main_domain.tld.clone(),
        //     owner: main_domain.name_account.to_string(),
        // });
        // }

        Ok(())
    }
}
