* Handle the 0 collateral case by wiping out balances
* Refactor code by extracting large, deeply nested blocks to their own functions
    * require_some token comparison
    * query.rs's Balance arm
