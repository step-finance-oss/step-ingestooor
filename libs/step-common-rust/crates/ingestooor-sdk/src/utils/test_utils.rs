use std::rc::Rc;

use crate::{structs::simple_mem_cache::SimpleMemCache, trait_defs::cache::TimeSeriesCacheLike};

use super::step_utils::StepUtils;

/// Initializes the `cache`, `step_utils`, and `rpc_client` for use in tests, intended to NOT be used.
/// This is just a shorthand for initializing these objects
pub async fn init_test_extras_bare() -> (Rc<StepUtils>, SimpleMemCache) {
    let utils = Rc::new(StepUtils::new(None));
    let cache = SimpleMemCache::new(None).await;
    (utils, cache)
}
