use crate::prelude::reply::{ReplyId, EPHEMERAL_BONUS_FUND};
use crate::prelude::*;
use crate::state::farming::RawFarmerStats;
use cosmwasm_std::{to_binary, BankMsg, CosmosMsg, SubMsg, WasmMsg};
use cw_storage_plus::Item;
use msg::contracts::market::entry::LpInfoResp;
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

/// Bonus fund configuration info
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct BonusConfig {
    /// The part of the reinvested yield that goes to the [BONUS_FUND]
    pub(crate) ratio: Decimal256,
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

    /// Calculates emissions per farming tokens ratio since the last update
    pub fn calculate_emissions_per_token_per_time(
        &self,
        store: &dyn Storage,
        emissions: &Emissions,
    ) -> Result<LvnToken> {
        let emissions_duration = Decimal256::from_ratio(
            emissions
                .end
                .checked_sub(emissions.start, "emissions_duration")?
                .as_nanos(),
            1u64,
        );
        let (latest_timestamp, latest_rewards_per_token) =
            self.latest_emissions_per_token(store)?;
        let total_farming_tokens = self.load_farming_totals(store)?.farming;

        let rewards_per_token = if total_farming_tokens.is_zero() {
            LvnToken::zero()
        } else {
            let total_lvn = emissions.lvn.raw();
            let start_time = max(latest_timestamp, emissions.start);
            let end_time = min(self.now(), emissions.end);
            let elapsed_time =
                end_time.checked_sub(start_time, "calculate_rewards_per_farming_token")?;
            let elapsed_time = Decimal256::from_ratio(elapsed_time.as_nanos(), 1u64);
            let elapsed_ratio = elapsed_time.checked_div(emissions_duration)?;

            total_lvn
                .checked_div_dec(total_farming_tokens.into_decimal256())?
                .checked_mul_dec(elapsed_ratio)?
        };

        let new_rewards_per_token = latest_rewards_per_token.checked_add(rewards_per_token)?;

        Ok(new_rewards_per_token)
    }

    /// Performs bookkeeping on any pending values for a farmer
    pub(crate) fn farming_perform_emissions_bookkeeping(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        farmer_stats: &mut RawFarmerStats,
    ) -> Result<()> {
        if let Some(emissions) = self.may_load_lvn_emissions(ctx.storage)? {
            self.update_emissions_per_token(ctx, &emissions)?;
            self.update_accrued_emissions(ctx, addr, &emissions, farmer_stats)?;
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
    ) -> Result<()> {
        let rewards_per_token =
            self.calculate_emissions_per_token_per_time(ctx.storage, emissions)?;

        EMISSIONS_PER_TIME_PER_TOKEN.save(ctx.storage, self.now(), &rewards_per_token)?;

        Ok(())
    }

    /// Allocates accrued emissions to the specified user
    pub(crate) fn update_accrued_emissions(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        emissions: &Emissions,
        farmer_stats: &mut RawFarmerStats,
    ) -> Result<()> {
        let accrued_rewards =
            self.calculate_unlocked_emissions(ctx.storage, farmer_stats, emissions)?;

        farmer_stats.accrued_emissions = farmer_stats
            .accrued_emissions
            .checked_add(accrued_rewards)?;
        let end_prefix_sum = self.calculate_emissions_per_token_per_time(ctx.storage, emissions)?;
        farmer_stats.xlp_last_claimed_prefix_sum = end_prefix_sum;

        self.save_raw_farmer_stats(ctx.storage, addr, farmer_stats)?;

        Ok(())
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

    pub(crate) fn reinvest_yield(&self, ctx: &mut StateContext) -> Result<()> {
        let lp_info: LpInfoResp = self.querier.query_wasm_smart(
            self.market_info.addr.clone(),
            &LpInfo {
                liquidity_provider: self.env.contract.address.clone().into(),
            },
        )?;
        let config = self.load_bonus_config(ctx.storage)?;
        let bonus_amount = lp_info.available_yield.checked_mul_dec(config.ratio)?;
        let reinvest_amount = lp_info
            .available_yield
            .checked_sub(bonus_amount)
            .map(NonZero::<Collateral>::new)?;

        if reinvest_amount.is_some() {
            let token = self.market_info.collateral.clone();
            let bonus_amount = token
                .into_u128(bonus_amount.into_decimal256())?
                .with_context(|| format!("unable to convert bonus_amount {:?}", bonus_amount))?;
            let bonus_amount = token
                .from_u128(bonus_amount)
                .map(Collateral::from_decimal256)?;
            let reinvest_msg = WasmMsg::Execute {
                contract_addr: self.market_info.addr.to_string(),
                msg: to_binary(&ReinvestYield {
                    stake_to_xlp: true,
                    amount: reinvest_amount,
                })?,
                funds: vec![],
            };

            EPHEMERAL_BONUS_FUND.save(ctx.storage, &bonus_amount)?;

            ctx.response.add_raw_submessage(SubMsg::reply_on_success(
                reinvest_msg,
                ReplyId::ReinvestYield.into(),
            ));
        }

        Ok(())
    }

    pub(crate) fn handle_reinvest_yield_reply(&self, store: &mut dyn Storage) -> Result<()> {
        let expected_yield = EPHEMERAL_BONUS_FUND.load_once(store)?;
        let balance = self
            .market_info
            .collateral
            .query_balance(&self.querier, &self.env.contract.address)?;

        anyhow::ensure!(
            expected_yield <= balance,
            "expected yield {} is greater than the current balance {}",
            expected_yield,
            balance
        );

        let mut fund_balance = self.load_bonus_fund(store)?;
        fund_balance = fund_balance.checked_add(expected_yield)?;

        anyhow::ensure!(
            fund_balance <= balance,
            "bonus fund {} is greater than the current balance {}",
            expected_yield,
            balance
        );

        self.save_bonus_fund(store, fund_balance)?;

        let xlp_balance = self.query_xlp_balance()?;
        let mut totals = self.load_farming_totals(store)?;
        totals.xlp = xlp_balance;
        self.save_farming_totals(store, &totals)?;

        Ok(())
    }

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
}
