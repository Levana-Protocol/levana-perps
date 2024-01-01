# Crank/price bot workflow

* Query the status endpoint of the contract
* If there are deferred work items or crank work items:
  * Get the deferred work item timestamp and compare to latest publish time in the on-chain oracle
    * Note: that timestamp need not be pushed to the market contract already
    * If the publish time on-chain is earlier than the deferred work item:
      * If we have the off-chain price attestation that is later than deferred work item, set both "perform price update" and "perform crank" to true
    * If the publish time on-chain is later than the deferred work itme, set "perform crank" to true
* If there are crank work items available, set "perform crank" to true
* If the last price update in the oracle is more than 1 hour old or price delta is more than 1%:
  * Set "perform crank" and "perform price update" to true
  * This will ensure that liquifundings don't queue up too much, but is otherwise not strictly necessary for the protocol
* Check if the latest price update from the off-chain oracle will hit any trigger prices
  * If so, set "perform price update" and "perform crank" to true
* If "perform price update" is true, perform an on-chain oracle update
* If "perform price update" or "perform crank" is true, perform a crank
  * Assertions: if "perform price update" is true, then "perform crank" must also be true
