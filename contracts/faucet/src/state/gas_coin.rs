use perpswap::contracts::faucet::entry::GasAllowance;

use super::*;

const GAS_ALLOWANCE: Item<GasAllowance> = Item::new("gas-allowance");

pub(crate) fn get_gas_allowance(store: &dyn Storage) -> Result<Option<GasAllowance>> {
    GAS_ALLOWANCE.may_load(store).map_err(|e| e.into())
}

pub(crate) fn set_gas_allowance(
    store: &mut dyn Storage,
    gas_allowance: &GasAllowance,
) -> Result<()> {
    GAS_ALLOWANCE
        .save(store, gas_allowance)
        .map_err(|e| e.into())
}

pub(crate) fn clear_gas_allowance(store: &mut dyn Storage) {
    GAS_ALLOWANCE.remove(store);
}
