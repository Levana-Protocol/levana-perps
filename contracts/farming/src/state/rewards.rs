use crate::prelude::farming::FarmingTotals;
use crate::prelude::reply::{ReplyId, EPHEMERAL_BONUS_FUND};
use crate::prelude::*;
use crate::state::farming::RawFarmerStats;
use crate::state::reply::BonusFundReplyData;
use anyhow::ensure;
use cosmwasm_std::{to_json_binary, BankMsg, CosmosMsg, SubMsg, WasmMsg};
use cw_storage_plus::Item;
use msg::contracts::market::entry::LpInfoResp;
use msg::prelude::ratio::InclusiveRatio;
use msg::prelude::MarketExecuteMsg::ReinvestYield;
use msg::prelude::MarketQueryMsg::LpInfo;
use msg::token::Token;
use std::cmp::{max, min};

/// The LVN token used for rewards
const LVN_TOKEN: Item<Token> = Item::new(namespace::LVN_TOKEN);

/// Tracks how much reward is allocated per token at a given timestamp
///
/// REWARDS_PER_TIME_PER_TOKEN is structured as a prefix sum data series where each value in the map
/// represents part of a formula that is used to calculate how many reward tokens a farmer gets at
/// any given time. The value is calculated as follows:
///
/// total_rewards / total_farming_tokens * elapsed_ratio
///
/// where
/// * total_rewards - the total amount of LVN included in the current emissions period
/// * total_farming_tokens - the total amount of farming tokens currently minted (see [RawFarmerStats])
/// * elapsed_ratio - the amount of time that has elapsed since the last entry relative to the duration
///                   of the current emissions period
///
/// When the amount of farming tokens changes (i.e. on a deposit or withdrawal), this value is added
/// to the previous entry and inserted into the Map, thereby allowing us to calculate the value of
/// a farming token for any given interval.
const EMISSIONS_PER_TIME_PER_TOKEN: Map<Timestamp, LvnToken> =
    Map::new(namespace::REWARDS_PER_TIME_PER_TOKEN);

/// The active LVN emission plan
const LVN_EMISSIONS: Item<Emissions> = Item::new(namespace::LVN_EMISSIONS);

/// If an emissions period is cancelled in the middle (via [ClearEmissions]) this keeps track of the
/// leftover LVN tokens that can be reclaimed.
const RECLAIMABLE_EMISSIONS: Item<LvnToken> = Item::new(namespace::RECLAIMABLE_EMISSIONS);

/// Lockdrop configuration info
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct LockdropConfig {
    /// The amount of time in seconds it takes for lockdrop rewards to unlock
    pub(crate) lockdrop_lvn_unlock_seconds: Duration,
    /// The ratio of lockdrop rewards that are immediately available on launch
    pub(crate) lockdrop_immediate_unlock_ratio: Decimal256,
}

/// The Bonus Fund contains funds that come from a portion of reinvested yield.
///
/// The farming contract allows liquidity providers to deposit xLP and receive a portion of LVN
/// emissions in return. However, they lose direct access to yield that comes from trading activity.
/// The farming contract, which controls all of the deposited xLP, has a mechanism to reinvest
/// accrued yield and distribute it amongst farmers (see [ExecuteMsg::Reinvest]). A portion of this
/// yield is put aside in a special fund called the Bonus Fund to be used at a later time.
const BONUS_FUND: Item<Collateral> = Item::new(namespace::BONUS_FUND);

/// It's possible for there to be no deposits during an emissions period. When this happens, the
/// tokens that are emitted during that time interval are able to be reclaimed by the protocol. This
/// Item tracks when such a period begins.
const RECLAIMABLE_START: Item<Timestamp> = Item::new(namespace::RECLAIMABLE_START);

/// Bonus fund configuration info
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct BonusConfig {
    /// The part of the reinvested yield that goes to the [BONUS_FUND]
    pub(crate) ratio: InclusiveRatio,
    /// The destination for the funds collected in the [BONUS_FUND]
    pub(crate) addr: Addr,
}

const LOCKDROP_CONFIG: Item<LockdropConfig> = Item::new(namespace::LOCKDROP_CONFIG);
const BONUS_CONFIG: Item<BonusConfig> = Item::new(namespace::BONUS_CONFIG);

impl State<'_> {
    pub(crate) fn rewards_init(
        &self,
        store: &mut dyn Storage,
        lvn_token_denom: &str,
    ) -> Result<()> {
        self.save_lvn_token(store, lvn_token_denom.to_string())?;
        self.save_bonus_fund(store, Collateral::zero())?;
        self.save_reclaimable_emissions(store, LvnToken::zero())?;

        EMISSIONS_PER_TIME_PER_TOKEN
            .save(store, self.now(), &LvnToken::zero())
            .map_err(|e| e.into())
    }

    fn save_lvn_token(&self, store: &mut dyn Storage, denom: String) -> Result<()> {
        let token = Token::Native {
            denom,
            decimal_places: 6,
        };

        LVN_TOKEN.save(store, &token)?;
        Ok(())
    }

    pub(crate) fn load_lvn_token(&self, store: &dyn Storage) -> Result<Token> {
        let token = LVN_TOKEN.load(store)?;
        Ok(token)
    }

    pub(crate) fn save_lvn_emissions(
        &self,
        store: &mut dyn Storage,
        emissions: Option<Emissions>,
    ) -> Result<()> {
        match emissions {
            None => LVN_EMISSIONS.remove(store),
            Some(emissions) => LVN_EMISSIONS.save(store, &emissions)?,
        }

        Ok(())
    }

    pub(crate) fn may_load_lvn_emissions(&self, store: &dyn Storage) -> Result<Option<Emissions>> {
        let emissions = LVN_EMISSIONS.may_load(store)?;
        Ok(emissions)
    }

    pub(crate) fn save_lockdrop_config(
        &self,
        store: &mut dyn Storage,
        config: LockdropConfig,
    ) -> Result<()> {
        LOCKDROP_CONFIG.save(store, &config)?;
        Ok(())
    }

    pub(crate) fn load_lockdrop_config(&self, store: &dyn Storage) -> Result<LockdropConfig> {
        let lockdrop_config = LOCKDROP_CONFIG.load(store)?;
        Ok(lockdrop_config)
    }

    pub(crate) fn save_bonus_config(
        &self,
        store: &mut dyn Storage,
        config: BonusConfig,
    ) -> Result<()> {
        BONUS_CONFIG.save(store, &config)?;
        Ok(())
    }

    pub(crate) fn load_bonus_config(&self, store: &dyn Storage) -> Result<BonusConfig> {
        let bonus_config = BONUS_CONFIG.load(store)?;
        Ok(bonus_config)
    }

    pub(crate) fn load_bonus_fund(&self, store: &dyn Storage) -> Result<Collateral> {
        let fund = BONUS_FUND.load(store)?;
        Ok(fund)
    }

    pub(crate) fn save_bonus_fund(
        &self,
        store: &mut dyn Storage,
        amount: Collateral,
    ) -> Result<()> {
        BONUS_FUND.save(store, &amount)?;
        Ok(())
    }

    pub(crate) fn load_reclaimable_emissions(&self, store: &dyn Storage) -> Result<LvnToken> {
        let amount = RECLAIMABLE_EMISSIONS.load(store)?;
        Ok(amount)
    }

    pub(crate) fn save_reclaimable_emissions(
        &self,
        store: &mut dyn Storage,
        amount: LvnToken,
    ) -> Result<()> {
        RECLAIMABLE_EMISSIONS.save(store, &amount)?;
        Ok(())
    }

    pub(crate) fn save_reclaimable_start(
        &self,
        store: &mut dyn Storage,
        start_time: Option<Timestamp>,
    ) -> Result<()> {
        match start_time {
            None => RECLAIMABLE_START.remove(store),
            Some(start_time) => RECLAIMABLE_START.save(store, &start_time)?,
        }

        Ok(())
    }

    pub(crate) fn may_load_reclaimable_start(
        &self,
        store: &dyn Storage,
    ) -> Result<Option<Timestamp>> {
        let start_time = RECLAIMABLE_START.may_load(store)?;
        Ok(start_time)
    }

    /// Calculates how many reward tokens can be claimed from the lockdrop and transfers them to the
    /// specified user
    pub(crate) fn claim_lockdrop_rewards(&self, ctx: &mut StateContext, user: &Addr) -> Result<()> {
        let mut farmer_stats = match self.load_raw_farmer_stats(ctx.storage, user)? {
            None => bail!("Unable to claim rewards, {} does not exist", user),
            Some(stats) => stats,
        };

        let unlocked =
            self.calculate_unlocked_lockdrop_rewards(ctx.storage, user, &farmer_stats)?;
        let amount = NumberGtZero::new(unlocked.into_decimal256());

        match amount {
            None => Ok(()),
            Some(amount) => {
                farmer_stats.lockdrop_amount_claimed =
                    farmer_stats.lockdrop_amount_claimed.checked_add(unlocked)?;
                farmer_stats.lockdrop_last_claimed = Some(self.now());
                self.save_raw_farmer_stats(ctx.storage, user, &farmer_stats)?;

                let coin = self
                    .load_lvn_token(ctx.storage)?
                    .into_native_coin(amount)?
                    .context("Invalid LVN transfer amount calculated")?;

                let transfer_msg = CosmosMsg::Bank(BankMsg::Send {
                    to_address: user.to_string(),
                    amount: vec![coin],
                });

                ctx.response.add_message(transfer_msg);

                Ok(())
            }
        }
    }

    /// Get the latest key and value from [REWARDS_PER_TIME_PER_TOKEN].
    fn latest_emissions_per_token(&self, store: &dyn Storage) -> Result<(Timestamp, LvnToken)> {
        EMISSIONS_PER_TIME_PER_TOKEN
            .range(store, None, None, Order::Descending)
            .next()
            .expect("REWARDS_PER_TIME_PER_TOKEN cannot be empty")
            .map_err(|e| e.into())
    }

    /// Determines whether a time interval has elapsed where there were no farmers and, if so, tracks
    /// the corresponding emissions accordingly.
    ///
    /// Returns the amount of reclaimable emissions
    pub(crate) fn process_reclaimable_emissions(
        &self,
        store: &mut dyn Storage,
    ) -> Result<LvnToken> {
        let reclaimable_start = self.may_load_reclaimable_start(store)?;
        let mut reclaimable = self.load_reclaimable_emissions(store)?;

        if let Some(reclaimable_start) = reclaimable_start {
            let emissions = self
                .may_load_lvn_emissions(store)?
                .context("Unable to find emissions when processing reclaim")?;
            let end_time = min(self.now(), emissions.end);
            let elapsed =
                end_time.checked_sub(reclaimable_start, "process_reclaimable_emissions-1")?;

            if elapsed.as_nanos() > 0 {
                let elapsed = Decimal256::from_ratio(elapsed.as_nanos(), 1u64);
                let duration = emissions
                    .end
                    .checked_sub(emissions.start, "process_reclaimable_emissions-2")?;
                let duration = Decimal256::from_ratio(duration.as_nanos(), 1u64);
                let new_reclaimable_emissions = emissions
                    .lvn
                    .raw()
                    .checked_mul_dec(elapsed)?
                    .checked_div_dec(duration)?;

                reclaimable = reclaimable.checked_add(new_reclaimable_emissions)?;
                self.save_reclaimable_emissions(store, reclaimable)?;
            }
        }

        Ok(reclaimable)
    }

    /// Calculates emissions per farming tokens ratio since the last update
    pub(crate) fn calculate_emissions_per_token_per_time(
        &self,
        store: &dyn Storage,
        emissions: &Emissions,
    ) -> Result<LvnToken> {
        let emissions_duration_nanos = Decimal256::from_ratio(
            emissions
                .end
                .checked_sub(emissions.start, "emissions_duration")?
                .as_nanos(),
            1u64,
        );
        let (latest_timestamp, latest_emissions_per_token) =
            self.latest_emissions_per_token(store)?;
        let total_farming_tokens = self.load_farming_totals(store)?.farming;

        let emissions_per_token = if total_farming_tokens.is_zero() {
            LvnToken::zero()
        } else {
            let total_lvn = emissions.lvn.raw();
            let start_time = max(latest_timestamp, emissions.start);
            let end_time = min(self.now(), emissions.end);
            let elapsed_time =
                end_time.checked_sub(start_time, "calculate_emissions_per_token_per_time")?;
            let elapsed_time = Decimal256::from_ratio(elapsed_time.as_nanos(), 1u64);
            let elapsed_ratio = elapsed_time.checked_div(emissions_duration_nanos)?;

            total_lvn
                .checked_mul_dec(elapsed_ratio)?
                .checked_div_dec(total_farming_tokens.into_decimal256())?
        };

        let new_emissions_per_token =
            latest_emissions_per_token.checked_add(emissions_per_token)?;

        Ok(new_emissions_per_token)
    }

    /// Performs bookkeeping on any pending values for a farmer
    pub(crate) fn farming_perform_emissions_bookkeeping(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        farmer_stats: &mut RawFarmerStats,
    ) -> Result<()> {
        if let Some(emissions) = self.may_load_lvn_emissions(ctx.storage)? {
            let emissions_per_token = self.update_emissions_per_token(ctx, &emissions)?;
            let accrued_rewards =
                self.calculate_unlocked_emissions(ctx.storage, farmer_stats, &emissions)?;

            farmer_stats.accrued_emissions = farmer_stats
                .accrued_emissions
                .checked_add(accrued_rewards)?;
            farmer_stats.xlp_last_claimed_prefix_sum = emissions_per_token;

            self.save_raw_farmer_stats(ctx.storage, addr, farmer_stats)?;
        }

        Ok(())
    }

    /// Calculates the amount of unlocked emissions that are available from the last time emissions
    /// were claimed or accrued
    pub(crate) fn calculate_unlocked_emissions(
        &self,
        store: &dyn Storage,
        farmer_stats: &RawFarmerStats,
        emissions: &Emissions,
    ) -> Result<LvnToken> {
        let start_prefix_sum = farmer_stats.xlp_last_claimed_prefix_sum;
        let end_prefix_sum = self.calculate_emissions_per_token_per_time(store, emissions)?;
        let unlocked_emissions = end_prefix_sum
            .checked_sub(start_prefix_sum)?
            .checked_mul_dec(farmer_stats.farming_tokens.into_decimal256())?;

        Ok(unlocked_emissions)
    }

    /// Updates the emissions per farming token ratio
    pub(crate) fn update_emissions_per_token(
        &self,
        ctx: &mut StateContext,
        emissions: &Emissions,
    ) -> Result<LvnToken> {
        let emissions_per_token =
            self.calculate_emissions_per_token_per_time(ctx.storage, emissions)?;
        let key = min(self.now(), emissions.end);

        EMISSIONS_PER_TIME_PER_TOKEN.save(ctx.storage, key, &emissions_per_token)?;

        Ok(emissions_per_token)
    }

    /// Calculates how many tokens the user can claim from LVN emissions
    /// and transfers them to the specified user
    pub(crate) fn claim_lvn_emissions(&self, ctx: &mut StateContext, addr: &Addr) -> Result<()> {
        let mut farmer_stats = match self.load_raw_farmer_stats(ctx.storage, addr)? {
            None => bail!("Unable to claim emissions, {} does not exist", addr),
            Some(farmer_stats) => farmer_stats,
        };

        let emissions = self.may_load_lvn_emissions(ctx.storage)?;
        let unlocked_rewards = match emissions {
            None => LvnToken::zero(),
            Some(emissions) => {
                if farmer_stats.farming_tokens.is_zero() {
                    LvnToken::zero()
                } else {
                    let unlocked =
                        self.calculate_unlocked_emissions(ctx.storage, &farmer_stats, &emissions)?;
                    let end_prefix_sum =
                        self.calculate_emissions_per_token_per_time(ctx.storage, &emissions)?;

                    farmer_stats.xlp_last_claimed_prefix_sum = end_prefix_sum;

                    unlocked
                }
            }
        };

        let amount = unlocked_rewards.checked_add(farmer_stats.accrued_emissions)?;

        farmer_stats.accrued_emissions = LvnToken::zero();
        self.save_raw_farmer_stats(ctx.storage, addr, &farmer_stats)?;

        let amount = NumberGtZero::new(amount.into_decimal256())
            .context("There are no unclaimed emissions")?;
        let coin = self
            .load_lvn_token(ctx.storage)?
            .into_native_coin(amount)?
            .context("Invalid LVN transfer amount calculated")?;

        let transfer_msg = CosmosMsg::Bank(BankMsg::Send {
            to_address: addr.to_string(),
            amount: vec![coin],
        });

        ctx.response.add_message(transfer_msg);

        Ok(())
    }

    /// Reinvests yield that was accrued from holding xLP tokens
    pub(crate) fn reinvest_yield(&self, ctx: &mut StateContext) -> Result<()> {
        let lp_info: LpInfoResp = self.querier.query_wasm_smart(
            self.market_info.addr.clone(),
            &LpInfo {
                liquidity_provider: self.env.contract.address.clone().into(),
            },
        )?;
        let config = self.load_bonus_config(ctx.storage)?;
        let bonus_amount = lp_info
            .available_yield
            .checked_mul_dec(config.ratio.raw())?;
        let reinvest_amount = lp_info
            .available_yield
            .checked_sub(bonus_amount)
            .map(NonZero::<Collateral>::new)?;

        if let Some(reinvest_amount) = reinvest_amount {
            let token = self.market_info.collateral.clone();
            let bonus_amount = token
                .into_u128(bonus_amount.into_decimal256())?
                .with_context(|| format!("unable to convert bonus_amount {:?}", bonus_amount))?;
            let bonus_amount = token
                .from_u128(bonus_amount)
                .map(Collateral::from_decimal256)?;
            let reinvest_msg = WasmMsg::Execute {
                contract_addr: self.market_info.addr.to_string(),
                msg: to_json_binary(&ReinvestYield {
                    stake_to_xlp: true,
                    amount: Some(reinvest_amount),
                })?,
                funds: vec![],
            };
            let xlp_before_reinvest = self.query_xlp_balance()?;

            EPHEMERAL_BONUS_FUND.save(
                ctx.storage,
                &BonusFundReplyData {
                    bonus_amount,
                    reinvest_amount: reinvest_amount.raw(),
                    xlp_before_reinvest,
                },
            )?;

            ctx.response.add_raw_submessage(SubMsg::reply_on_success(
                reinvest_msg,
                ReplyId::ReinvestYield.into(),
            ));
        }

        Ok(())
    }

    /// Reply handler for Reinvest Yield
    pub(crate) fn handle_reinvest_yield_reply(&self, ctx: &mut StateContext) -> Result<()> {
        let ephemeral_data = EPHEMERAL_BONUS_FUND.load_once(ctx.storage)?;
        let balance = self
            .market_info
            .collateral
            .query_balance(&self.querier, &self.env.contract.address)?;

        anyhow::ensure!(
            ephemeral_data.bonus_amount <= balance,
            "expected yield {} is greater than the current balance {}",
            ephemeral_data.bonus_amount,
            balance
        );

        let mut fund_balance = self.load_bonus_fund(ctx.storage)?;
        fund_balance = fund_balance.checked_add(ephemeral_data.bonus_amount)?;

        anyhow::ensure!(
            fund_balance <= balance,
            "bonus fund {} is greater than the current balance {}",
            ephemeral_data.bonus_amount,
            balance
        );

        self.save_bonus_fund(ctx.storage, fund_balance)?;

        let xlp_balance = self.query_xlp_balance()?;
        let mut totals = self.load_farming_totals(ctx.storage)?;
        let xlp_delta = xlp_balance.checked_sub(ephemeral_data.xlp_before_reinvest)?;

        totals.xlp = xlp_balance;
        self.save_farming_totals(ctx.storage, &totals)?;

        ctx.response.add_event(ReinvestEvent {
            reinvested_yield: ephemeral_data.reinvest_amount,
            xlp: xlp_delta,
            bonus_yield: ephemeral_data.bonus_amount,
        });

        ctx.response.add_event(FarmingPoolSizeEvent {
            farming: totals.farming,
            xlp: totals.xlp,
        });

        Ok(())
    }

    /// Transfers the bonus fund to the designated addr
    pub(crate) fn transfer_bonus(&self, ctx: &mut StateContext) -> Result<()> {
        let config = self.load_bonus_config(ctx.storage)?;
        let amount = self
            .load_bonus_fund(ctx.storage)
            .map(NonZero::<Collateral>::new)?;

        if let Some(amount) = amount {
            self.save_bonus_fund(ctx.storage, Collateral::zero())?;

            let transfer_msg = self
                .market_info
                .collateral
                .into_transfer_msg(&config.addr, amount)?
                .with_context(|| "unable to construct msg to transfer bonus")?;

            ctx.response.add_message(transfer_msg);
        }

        Ok(())
    }

    pub(crate) fn update_reclaimable_start(
        &self,
        store: &mut dyn Storage,
        emissions: Option<Emissions>,
        totals: Option<FarmingTotals>,
    ) -> Result<()> {
        match (emissions, totals) {
            (Some(emissions), Some(totals)) => {
                if totals.farming.is_zero() {
                    // handle case where there are no deposits and...

                    if self.now() < emissions.start {
                        // ...emissions period starts in the future

                        self.save_reclaimable_start(store, Some(emissions.start))?;
                    } else if self.now() < emissions.end {
                        // ...emissions period is active

                        self.save_reclaimable_start(store, Some(self.now()))?;
                    } else {
                        // ...emissions period ended in the past

                        self.save_reclaimable_start(store, None)?;
                    }
                } else {
                    // handle case where there are deposits

                    self.save_reclaimable_start(store, None)?;
                }
            }
            _ => self.save_reclaimable_start(store, None)?,
        }

        Ok(())
    }

    /// Transfers LVN tokens leftover from an emissions.
    /// There are two scenarios where this can occur
    ///
    /// 1. If the emissions period is terminated prematurely ([OwnerExecuteMsg::ClearEmissions])
    /// 2. If at any point during an emissions period there is no collateral deposited
    pub(crate) fn reclaim_emissions(
        &self,
        ctx: &mut StateContext,
        addr: Addr,
        amount: Option<LvnToken>,
    ) -> Result<()> {
        let reclaimable = self.process_reclaimable_emissions(ctx.storage)?;
        let totals = self.load_farming_totals(ctx.storage)?;
        let emissions = self.may_load_lvn_emissions(ctx.storage)?;

        self.update_reclaimable_start(ctx.storage, emissions, Some(totals))?;

        ensure!(
            reclaimable > LvnToken::zero(),
            "There are no emissions to reclaim"
        );

        let amount = match amount {
            None => reclaimable,
            Some(amount) => {
                ensure!(
                    amount <= reclaimable,
                    "Error reclaiming emissions, requested: {}, available: {}",
                    amount,
                    reclaimable
                );

                amount
            }
        };

        let remaining_reclaimable = reclaimable.checked_sub(amount)?;
        self.save_reclaimable_emissions(ctx.storage, remaining_reclaimable)?;

        let amount = NumberGtZero::new(amount.into_decimal256())
            .context("Unable to convert amount into NumberGtZero")?;
        let coin = self
            .load_lvn_token(ctx.storage)?
            .into_native_coin(amount)?
            .context("Invalid LVN transfer amount calculated")?;
        let transfer_msg = CosmosMsg::Bank(BankMsg::Send {
            to_address: addr.to_string(),
            amount: vec![coin],
        });

        ctx.response.add_message(transfer_msg);

        Ok(())
    }
}
