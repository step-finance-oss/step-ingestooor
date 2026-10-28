use itertools::Itertools;
use std::collections::HashMap;

use log::{debug, info};
use step_ingestooor_parsers::{
    construct_all_parsers, TransactionParser, TransactionParserFilter, TransactionParserTrait,
};
use step_ingestooor_sdk::{
    decoded_transaction::ProgramAndAccountIdsProvider, utils::types::ImmutableVec,
};

use crate::slot_stats::SlotStatsParser;

pub struct ParserManager {
    // slot_stats_parser: SlotStatsParser,
    //this vector must not be mutated after creation since the vectors of usize are indexes
    parser_infos: ImmutableVec<InnerParserInfo>,
    program_id_to_parser: HashMap<String, Vec<usize>>,
    account_id_to_parser: HashMap<String, Vec<usize>>,
    parsers_for_all_txs: Vec<TransactionParser>,
    slot_stats_parser: SlotStatsParser,
}

#[derive(Clone)]
struct InnerParserInfo {
    name: &'static str,
    parser: TransactionParser,
    filters: Vec<TransactionParserFilter>,
}

impl ParserManager {
    pub fn new() -> ParserManager {
        let parser_infos: Vec<InnerParserInfo> = construct_all_parsers()
            .into_iter()
            .map(|p| {
                let filters = p.get_filters();
                let name = p.get_name();
                if filters.is_empty() {
                    panic!("Parser {:?} has no filter", name);
                }
                InnerParserInfo {
                    parser: p,
                    name,
                    filters,
                }
            })
            .collect();

        let mut program_id_to_parser: HashMap<String, Vec<usize>> = HashMap::new();
        let mut account_id_to_parser: HashMap<String, Vec<usize>> = HashMap::new();
        let mut parsers_for_all_txs = Vec::new();
        for (idx, parser_info) in parser_infos.iter().enumerate() {
            debug!("Adding parser to lookup maps: {:?}", parser_info.name);
            for filter in parser_info.filters.iter() {
                match filter {
                    TransactionParserFilter::ProgramId(program_id) => {
                        program_id_to_parser
                            .entry(program_id.to_string())
                            .or_default()
                            .push(idx);
                    }
                    TransactionParserFilter::AccountId(account_id) => {
                        account_id_to_parser
                            .entry(account_id.to_string())
                            .or_default()
                            .push(idx);
                    }
                    TransactionParserFilter::None => {
                        parsers_for_all_txs.push(parser_info.parser.clone());
                    }
                }
            }
        }

        ParserManager {
            // slot_stats_parser: SlotStatsParser::new(),
            parser_infos: ImmutableVec::new(parser_infos),
            program_id_to_parser,
            account_id_to_parser,
            slot_stats_parser: SlotStatsParser::new(),
            parsers_for_all_txs,
        }
    }

    pub fn get_parsers_for_transaction<'a>(
        &'a self,
        tx: &'a impl ProgramAndAccountIdsProvider,
    ) -> impl Iterator<Item = &'a TransactionParser> + 'a {
        let program_parser_indexes = tx
            .get_program_ids()
            .into_iter()
            .filter_map(|a| self.program_id_to_parser.get(a))
            .flatten();
        let account_parser_indexes = tx
            .get_account_ids()
            .into_iter()
            .filter_map(|a| self.account_id_to_parser.get(a))
            .flatten();
        program_parser_indexes
            .chain(account_parser_indexes)
            .unique()
            .map(|a| &self.parser_infos[*a].parser)
            .chain(self.parsers_for_all_txs.iter())
    }

    pub fn get_slot_stats_parser(&self) -> &SlotStatsParser {
        &self.slot_stats_parser
    }
}

impl Default for ParserManager {
    fn default() -> Self {
        info!("Creating a default ParserManager");
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::parser_manager::ParserManager;
    use step_ingestooor_sdk::decoded_transaction::ProgramAndAccountIdsProvider;

    struct MockProgramAndAccountIdsProvider {
        program_ids: Vec<String>,
        account_ids: Vec<String>,
    }
    impl ProgramAndAccountIdsProvider for MockProgramAndAccountIdsProvider {
        fn get_program_ids(&self) -> Vec<&str> {
            self.program_ids.iter().map(|a| a.as_str()).collect()
        }
        fn get_account_ids(&self) -> Vec<&str> {
            self.account_ids.iter().map(|a| a.as_str()).collect()
        }
    }

    #[test]
    fn test_parser_manager_no_match() {
        let parser_manager = ParserManager::new();
        let tx = MockProgramAndAccountIdsProvider {
            program_ids: vec!["a".to_string(), "b".to_string()],
            account_ids: vec!["c".to_string(), "d".to_string()],
        };
        let parsers = parser_manager.get_parsers_for_transaction(&tx);
        assert_eq!(parsers.collect::<Vec<_>>().len(), 1);
    }

    #[test]
    fn test_parser_manager_match() {
        let parser_manager = ParserManager::new();
        let tx = MockProgramAndAccountIdsProvider {
            program_ids: vec!["JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string()],
            account_ids: vec![],
        };
        let parsers = parser_manager.get_parsers_for_transaction(&tx);
        assert_eq!(parsers.collect::<Vec<_>>().len(), 2);
    }

    #[test]
    fn test_parser_manager_match_unique() {
        let parser_manager = ParserManager::new();
        let tx = MockProgramAndAccountIdsProvider {
            program_ids: vec!["JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string()],
            account_ids: vec!["JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string()],
        };
        let parsers = parser_manager.get_parsers_for_transaction(&tx);
        assert_eq!(parsers.collect::<Vec<_>>().len(), 2);
    }
}
