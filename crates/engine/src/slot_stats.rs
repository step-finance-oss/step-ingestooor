use anyhow::anyhow;
use indexer_rabbitmq::geyser::SlotStatistics;
use step_ingestooor_sdk::{
    dooot::{slot_stats::SlotStatsGlobalDooot, Dooot},
    utils::util_fns::convert_blocktime_to_micros,
};

pub struct SlotStatsParser {}

impl SlotStatsParser {
    pub fn new() -> SlotStatsParser {
        SlotStatsParser {}
    }

    pub fn invoke(
        &self,
        slot_stats: SlotStatistics,
        dooots: &mut Vec<Dooot>,
    ) -> anyhow::Result<()> {
        let block_time = slot_stats
            .block_time
            .ok_or_else(|| anyhow!("missing block time in slot stats"))?;
        let converted_bt_micros =
            convert_blocktime_to_micros(block_time, slot_stats.slot_second_idx, None)?;

        let slot_stats_dooot = SlotStatsGlobalDooot {
            time: converted_bt_micros,
            slot: slot_stats.slot as i64,
            tx_success_count: slot_stats.tx_success as i32,
            tx_success_fees: slot_stats.tx_success_fees as i64,
            tx_success_fees_priority: slot_stats.tx_success_fees_priority as i64,
            tx_error_count: slot_stats.tx_err as i32,
            tx_error_fees: slot_stats.tx_err_fees as i64,
            tx_error_fees_priority: slot_stats.tx_err_fees_priority as i64,
            tx_vote_count: slot_stats.tx_vote as i32,
            tx_vote_fees: slot_stats.tx_vote_fees as i64,
            tx_vote_error_count: slot_stats.tx_vote_err as i32,
            tx_vote_error_fees: slot_stats.tx_vote_err_fees as i64,
            programs: slot_stats.programs.keys().map(|s| s.to_string()).collect(),
            program_success_counts: slot_stats
                .programs
                .values()
                .map(|v| v.success as i32)
                .collect(),
            program_error_counts: slot_stats
                .programs
                .values()
                .map(|v| v.failed as i32)
                .collect(),
            payers: slot_stats.payers.into_iter().collect(),
            mints: slot_stats
                .new_token_accounts
                .keys()
                .map(|s| s.to_string())
                .collect(),
            mint_new_account_counts: slot_stats
                .new_token_accounts
                .into_iter()
                .map(|v| v.1 as i32)
                .collect(),
            new_mints_fungible_count: slot_stats.mints_fungible_new as i32,
            new_mints_nonfungible_count: slot_stats.mints_nonfungible_new as i32,
            info: None,
        };
        dooots.push(Dooot::SlotStats(slot_stats_dooot));
        Ok(())
    }
}

impl Default for SlotStatsParser {
    fn default() -> Self {
        Self::new()
    }
}
