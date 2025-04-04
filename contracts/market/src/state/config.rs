use crate::state::*;
use anyhow::ensure;
use cw_storage_plus::Item;
use perpswap::contracts::market::{
    config::{Config, ConfigUpdate},
    spot_price::{
        PythConfig, SpotPriceConfig, SpotPriceConfigInit, SpotPriceFeed, SpotPriceFeedData,
        SpotPriceFeedDataInit, SpotPriceFeedInit, StrideConfig,
    },
};

const CONFIG_STORAGE: Item<Config> = Item::new(namespace::CONFIG);

pub(crate) fn load_config(store: &dyn Storage) -> Result<Config> {
    CONFIG_STORAGE.load(store).map_err(|e| e.into())
}

/// called only once, at instantiation
pub(crate) fn config_init(
    api: &dyn Api,
    store: &mut dyn Storage,
    config: Option<ConfigUpdate>,
    spot_price: SpotPriceConfigInit,
) -> Result<()> {
    let mut init_config = Config::new(convert_spot_price_init(api, spot_price)?);

    let update = config.unwrap_or_default();

    update_config(&mut init_config, api, store, update)?;

    Ok(())
}

pub(crate) fn update_config(
    config: &mut Config,
    api: &dyn Api,
    store: &mut dyn Storage,
    ConfigUpdate {
        trading_fee_notional_size,
        trading_fee_counter_collateral,
        crank_execs,
        max_leverage,
        carry_leverage,
        funding_rate_sensitivity,
        funding_rate_max_annualized,
        mute_events,
        liquifunding_delay_seconds,
        protocol_tax,
        unstake_period_seconds,
        target_utilization,
        borrow_fee_sensitivity,
        borrow_fee_rate_min_annualized,
        borrow_fee_rate_max_annualized,
        max_xlp_rewards_multiplier,
        min_xlp_rewards_multiplier,
        delta_neutrality_fee_sensitivity,
        delta_neutrality_fee_cap,
        delta_neutrality_fee_tax,
        crank_fee_charged,
        crank_fee_surcharge,
        crank_fee_reward,
        minimum_deposit_usd: minimum_deposit,
        liquifunding_delay_fuzz_seconds,
        max_liquidity,
        disable_position_nft_exec,
        liquidity_cooldown_seconds,
        exposure_margin_ratio,
        referral_reward_ratio,
        spot_price,
    }: ConfigUpdate,
) -> Result<()> {
    if let Some(x) = trading_fee_notional_size {
        config.trading_fee_notional_size = x;
    }

    if let Some(x) = trading_fee_counter_collateral {
        config.trading_fee_counter_collateral = x;
    }

    if let Some(x) = crank_execs {
        config.crank_execs = x;
    }

    if let Some(x) = max_leverage {
        config.max_leverage = x;
    }

    if let Some(x) = carry_leverage {
        config.carry_leverage = x;
    }

    if let Some(x) = funding_rate_max_annualized {
        config.funding_rate_max_annualized = x;
    }

    if let Some(x) = funding_rate_sensitivity {
        config.funding_rate_sensitivity = x;
    }

    if let Some(x) = mute_events {
        config.mute_events = x;
    }

    if let Some(x) = liquifunding_delay_seconds {
        config.liquifunding_delay_seconds = x;
    }

    if let Some(protocol_tax) = protocol_tax {
        config.protocol_tax = protocol_tax;
    }

    if let Some(x) = unstake_period_seconds {
        config.unstake_period_seconds = x;
    }

    if let Some(x) = target_utilization {
        config.target_utilization = x;
    }

    if let Some(x) = borrow_fee_sensitivity {
        config.borrow_fee_sensitivity = x;
    }
    if let Some(x) = borrow_fee_rate_min_annualized {
        config.borrow_fee_rate_min_annualized = x;
    }
    if let Some(x) = borrow_fee_rate_max_annualized {
        config.borrow_fee_rate_max_annualized = x;
    }
    if let Some(x) = max_xlp_rewards_multiplier {
        config.max_xlp_rewards_multiplier = x;
    }
    if let Some(x) = min_xlp_rewards_multiplier {
        config.min_xlp_rewards_multiplier = x;
    }

    if let Some(x) = delta_neutrality_fee_sensitivity {
        config.delta_neutrality_fee_sensitivity = x;
    }

    if let Some(x) = delta_neutrality_fee_cap {
        config.delta_neutrality_fee_cap = x;
    }

    if let Some(x) = delta_neutrality_fee_tax {
        config.delta_neutrality_fee_tax = x;
    }

    if let Some(x) = crank_fee_charged {
        config.crank_fee_charged = x;
    }
    if let Some(x) = crank_fee_surcharge {
        config.crank_fee_surcharge = x;
    }
    if let Some(x) = crank_fee_reward {
        config.crank_fee_reward = x;
    }
    if let Some(x) = minimum_deposit {
        config.minimum_deposit_usd = x;
    }
    if let Some(x) = liquifunding_delay_fuzz_seconds {
        config.liquifunding_delay_fuzz_seconds = x;
    }
    if let Some(x) = max_liquidity {
        config.max_liquidity = x;
    }
    if let Some(x) = disable_position_nft_exec {
        config.disable_position_nft_exec = x;
    }
    if let Some(x) = liquidity_cooldown_seconds {
        config.liquidity_cooldown_seconds = x;
    }

    if let Some(x) = spot_price {
        config.spot_price = convert_spot_price_init(api, x)?;
    }

    if let Some(x) = exposure_margin_ratio {
        config.exposure_margin_ratio = x;
    }

    if let Some(x) = referral_reward_ratio {
        config.referral_reward_ratio = x;
    }

    config.validate()?;

    CONFIG_STORAGE.save(store, config)?;

    Ok(())
}

pub(crate) fn convert_spot_price_init(
    api: &dyn Api,
    spot_price: SpotPriceConfigInit,
) -> Result<SpotPriceConfig> {
    Ok(match spot_price {
        SpotPriceConfigInit::Manual { admin } => SpotPriceConfig::Manual {
            admin: admin.validate(api)?,
        },
        SpotPriceConfigInit::Oracle {
            pyth,
            stride,
            feeds,
            feeds_usd,
            volatile_diff_seconds,
        } => {
            ensure!(!feeds.is_empty(), "feeds cannot be empty");
            ensure!(!feeds_usd.is_empty(), "feeds_usd cannot be empty");

            fn map_feeds(
                api: &dyn Api,
                feeds: Vec<SpotPriceFeedInit>,
            ) -> Result<Vec<SpotPriceFeed>> {
                feeds
                    .into_iter()
                    .map(|feed| {
                        Ok(SpotPriceFeed {
                            inverted: feed.inverted,
                            volatile: feed.volatile,
                            data: match feed.data {
                                SpotPriceFeedDataInit::Constant { price } => {
                                    SpotPriceFeedData::Constant { price }
                                }
                                SpotPriceFeedDataInit::Pyth {
                                    id,
                                    age_tolerance_seconds,
                                } => SpotPriceFeedData::Pyth {
                                    id,
                                    age_tolerance_seconds,
                                },
                                SpotPriceFeedDataInit::Stride {
                                    denom,
                                    age_tolerance_seconds,
                                } => SpotPriceFeedData::Stride {
                                    denom,
                                    age_tolerance_seconds,
                                },
                                SpotPriceFeedDataInit::Sei { denom } => {
                                    SpotPriceFeedData::Sei { denom }
                                }
                                SpotPriceFeedDataInit::Rujira { asset } => {
                                    SpotPriceFeedData::Rujira { asset }
                                }
                                SpotPriceFeedDataInit::Simple {
                                    contract,
                                    age_tolerance_seconds,
                                } => SpotPriceFeedData::Simple {
                                    contract: contract.validate(api)?,
                                    age_tolerance_seconds,
                                },
                            },
                        })
                    })
                    .collect()
            }

            SpotPriceConfig::Oracle {
                pyth: pyth
                    .map(|pyth| {
                        pyth.contract_address
                            .validate(api)
                            .map(|contract_address| PythConfig {
                                contract_address,
                                network: pyth.network,
                            })
                    })
                    .transpose()?,
                stride: stride
                    .map(|stride| {
                        stride
                            .contract_address
                            .validate(api)
                            .map(|contract_address| StrideConfig { contract_address })
                    })
                    .transpose()?,
                feeds: map_feeds(api, feeds)?,
                feeds_usd: map_feeds(api, feeds_usd)?,
                volatile_diff_seconds,
            }
        }
    })
}
