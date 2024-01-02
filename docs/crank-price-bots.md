# Crank/price bot workflow

* Query the status, oracle price, and "price will trigger" endpoints of the contract
* If the prices are not in the oracle at all for our market, return "update price and crank"
* If the last price update in the oracle is more than 1 hour old or price delta is more than 1%:
  * Return "update price and crank"
  * This will ensure that liquifundings don't queue up too much, but is otherwise not strictly necessary for the protocol
* If there are deferred work items:
  * Get the deferred work item timestamp and compare to latest publish time in the on-chain oracle
    * Note: that timestamp need not be pushed to the market contract already
    * If the publish time on-chain is earlier than the deferred work item:
      * If we have the off-chain price attestation that is later than deferred work item, return "update price and crank"
    * If the publish time on-chain is later than the deferred work time, return "just perform crank"
* If there are crank work items available, return "just perform crank"
* Check if the latest price update from the off-chain oracle will hit any trigger prices
  * If so, return "update price and crank"
  * Note: by placing this at the end of the list, we may end up skipping a price update because previous work is in the queue. This is intentional: we want to avoid repeatedly updating the price while processing non-triggered actions.
