use crate::state::*;
use anyhow::Context;
use cosmwasm_std::{Binary, Order, Uint128};
use cw_storage_plus::{Bound, Map};
use cw_utils::Expiration;
use msg::contracts::cw20::{
    entry::{
        AllAccountsResponse, AllAllowancesResponse, AllSpenderAllowancesResponse, AllowanceInfo,
        AllowanceResponse, BalanceResponse, DownloadLogoResponse, EmbeddedLogo, InstantiateMsg,
        Logo, LogoInfo, MarketingInfoResponse, MinterResponse, SpenderAllowanceInfo,
        TokenInfoResponse,
    },
    events::{
        AllowanceChangeEvent, AllowanceChangeKind, BurnEvent, LogoChangeEvent,
        MarketingChangeEvent, MintEvent, MinterChangeEvent, SendEvent, TransferEvent,
    },
    Cw20ReceiveMsg, ReceiverExecuteMsg,
};
use serde::{Deserialize, Serialize};
use shared::prelude::*;

pub(crate) const MINTER: Item<Addr> = Item::new(namespace::MINTER);
pub(crate) const MINTER_CAP: Item<Uint128> = Item::new(namespace::MINTER_CAP);
pub(crate) const TOKEN_INFO: Item<TokenInfo> = Item::new(namespace::TOKEN_INFO);
pub(crate) const MARKETING_INFO: Item<MarketingInfoResponse> = Item::new(namespace::MARKETING_INFO);
pub(crate) const LOGO: Item<Logo> = Item::new(namespace::LOGO);
pub(crate) const BALANCES: Map<&Addr, Uint128> = Map::new(namespace::BALANCES);
pub(crate) const ALLOWANCES: Map<(&Addr, &Addr), AllowanceResponse> =
    Map::new(namespace::ALLOWANCES);
pub(crate) const ALLOWANCES_SPENDER: Map<(&Addr, &Addr), AllowanceResponse> =
    Map::new(namespace::ALLOWANCES_SPENDER);

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
}

impl State<'_> {
    pub(crate) fn minter_resp(&self, store: &dyn Storage) -> Result<MinterResponse> {
        let addr = self.minter_addr(store)?;
        let cap = self.minter_cap(store)?;

        Ok(MinterResponse { minter: addr, cap })
    }

    pub(crate) fn minter_cap(&self, store: &dyn Storage) -> Result<Option<Uint128>> {
        MINTER_CAP.may_load(store).map_err(|err| err.into())
    }

    pub(crate) fn minter_addr(&self, store: &dyn Storage) -> Result<Addr> {
        MINTER.may_load(store)?.context("MINTER is not set")
    }

    pub(crate) fn balance(&self, store: &dyn Storage, addr: &Addr) -> Result<BalanceResponse> {
        let balance = BALANCES.may_load(store, addr)?.unwrap_or_default();
        Ok(BalanceResponse { balance })
    }

    pub(crate) fn token_info(&self, store: &dyn Storage) -> Result<TokenInfoResponse> {
        let info = TOKEN_INFO.load(store)?;
        let res = TokenInfoResponse {
            name: info.name,
            symbol: info.symbol,
            decimals: info.decimals,
            total_supply: info.total_supply,
        };
        Ok(res)
    }

    pub(crate) fn allowance(
        &self,
        store: &dyn Storage,
        owner: &Addr,
        spender: &Addr,
    ) -> Result<AllowanceResponse> {
        let allowance = ALLOWANCES
            .may_load(store, (owner, spender))?
            .unwrap_or_default();

        Ok(allowance)
    }

    pub(crate) fn owner_allowances(
        &self,
        store: &dyn Storage,
        owner: Addr,
        start_after: Option<Addr>,
        limit: Option<u32>,
    ) -> Result<AllAllowancesResponse> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start: Option<Bound<&Addr>> = start_after.as_ref().map(Bound::exclusive);

        let allowances = ALLOWANCES
            .prefix(&owner)
            .range(store, start, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                item.map(|(addr, allow)| AllowanceInfo {
                    spender: addr,
                    allowance: allow.allowance,
                    expires: allow.expires,
                })
                .map_err(|err| err.into())
            })
            .collect::<Result<_>>()?;

        Ok(AllAllowancesResponse { allowances })
    }

    pub(crate) fn spender_allowances(
        &self,
        store: &dyn Storage,
        spender: Addr,
        start_after: Option<Addr>,
        limit: Option<u32>,
    ) -> Result<AllSpenderAllowancesResponse> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start: Option<Bound<&Addr>> = start_after.as_ref().map(Bound::exclusive);

        let allowances = ALLOWANCES_SPENDER
            .prefix(&spender)
            .range(store, start, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                item.map(|(addr, allow)| SpenderAllowanceInfo {
                    owner: addr,
                    allowance: allow.allowance,
                    expires: allow.expires,
                })
                .map_err(|err| err.into())
            })
            .collect::<Result<_>>()?;
        Ok(AllSpenderAllowancesResponse { allowances })
    }

    pub(crate) fn all_accounts(
        &self,
        store: &dyn Storage,
        start_after: Option<Addr>,
        limit: Option<u32>,
    ) -> Result<AllAccountsResponse> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start: Option<Bound<&Addr>> = start_after.as_ref().map(Bound::exclusive);

        let accounts = BALANCES
            .keys(store, start, None, Order::Ascending)
            .take(limit)
            .map(|item| item.map(Into::into).map_err(|err| err.into()))
            .collect::<Result<_>>()?;

        Ok(AllAccountsResponse { accounts })
    }

    pub(crate) fn marketing_info(&self, store: &dyn Storage) -> Result<MarketingInfoResponse> {
        Ok(MARKETING_INFO.may_load(store)?.unwrap_or_default())
    }

    pub(crate) fn logo(&self, store: &dyn Storage) -> Result<DownloadLogoResponse> {
        let logo = LOGO.load(store)?;
        match logo {
            Logo::Embedded(EmbeddedLogo::Svg(logo)) => Ok(DownloadLogoResponse {
                mime_type: "image/svg+xml".to_owned(),
                data: logo,
            }),
            Logo::Embedded(EmbeddedLogo::Png(logo)) => Ok(DownloadLogoResponse {
                mime_type: "image/png".to_owned(),
                data: logo,
            }),
            Logo::Url(_) => Err(anyhow!("logo")),
        }
    }

    pub(crate) fn token_init(&self, ctx: &mut StateContext, msg: InstantiateMsg) -> Result<()> {
        msg.validate()?;

        let InstantiateMsg {
            name,
            symbol,
            decimals,
            initial_balances,
            minter,
            marketing,
        } = msg;

        // initial balances
        let mut total_supply = Uint128::zero();

        for row in initial_balances.iter() {
            let address = self.api.addr_validate(&row.address)?;
            BALANCES.save(ctx.storage, &address, &row.amount)?;
            total_supply += row.amount;
        }

        // minter
        MINTER.save(ctx.storage, &minter.minter.validate(self.api)?)?;

        if let Some(minter_cap) = minter.cap {
            MINTER_CAP.save(ctx.storage, &minter_cap)?;
            if total_supply > minter_cap {
                return Err(anyhow!("initial supply greater than cap"));
            }
        }

        // marketing and logo
        let mut init_marketing_info = MarketingInfoResponse::default();
        if let Some(marketing) = marketing.as_ref() {
            if let Some(x) = marketing.project.clone() {
                init_marketing_info.project = Some(x);
            }
            if let Some(x) = marketing.description.clone() {
                init_marketing_info.description = Some(x);
            }
            if let Some(x) = marketing.marketing.clone() {
                init_marketing_info.marketing = Some(x);
            }
            if let Some(x) = &marketing.logo {
                init_marketing_info.logo = Some(x.into());

                LOGO.save(ctx.storage, x)?;
            }
        }

        MARKETING_INFO.save(ctx.storage, &init_marketing_info)?;

        // token info
        let data = TokenInfo {
            name,
            symbol,
            decimals,
            total_supply,
        };
        TOKEN_INFO.save(ctx.storage, &data)?;

        Ok(())
    }

    pub(crate) fn transfer(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        recipient: Addr,
        amount: Uint128,
    ) -> Result<()> {
        ctx.response.add_event(TransferEvent {
            owner: owner.clone(),
            recipient: recipient.clone(),
            amount,
            by: None,
        });

        if amount == Uint128::zero() {
            return Err(anyhow!("amount cannot be zero"));
        }

        BALANCES.update(
            ctx.storage,
            &owner,
            |balance: Option<Uint128>| -> Result<_> {
                balance
                    .unwrap_or_default()
                    .checked_sub(amount)
                    .with_context(||
                        match balance {
                            None => format!("In CW20 contract, cannot transfer {amount}, have no funds for address {owner}"),
                            Some(balance) => format!("In CW20 contract, cannot transfer {amount}, {owner} only has {balance}")
                        }
                    )
            },
        )?;

        BALANCES.update(
            ctx.storage,
            &recipient,
            |balance: Option<Uint128>| -> Result<_> { Ok(balance.unwrap_or_default() + amount) },
        )?;

        Ok(())
    }

    pub(crate) fn transfer_from(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        owner: Addr,
        recipient: Addr,
        amount: Uint128,
    ) -> Result<()> {
        ctx.response.add_event(TransferEvent {
            owner: owner.clone(),
            recipient: recipient.clone(),
            amount,
            by: Some(sender.clone()),
        });

        // deduct allowance before doing anything else have enough allowance
        self.deduct_allowance(ctx, &owner, &sender, amount)?;

        BALANCES.update(
            ctx.storage,
            &owner,
            |balance: Option<Uint128>| -> Result<_> {
                balance.unwrap_or_default().checked_sub(amount)
                    .with_context(||
                        match balance {
                            None => format!("In CW20 contract, cannot send {amount}, have no funds for address {owner}"),
                            Some(balance) => format!("In CW20 contract, cannot send {amount}, {owner} only has {balance}")
                        }
                    )
            },
        )?;
        BALANCES.update(
            ctx.storage,
            &recipient,
            |balance: Option<Uint128>| -> Result<_> { Ok(balance.unwrap_or_default() + amount) },
        )?;

        Ok(())
    }

    pub(crate) fn burn(&self, ctx: &mut StateContext, owner: Addr, amount: Uint128) -> Result<()> {
        ctx.response.add_event(BurnEvent {
            owner: owner.clone(),
            amount,
            by: None,
        });

        if amount == Uint128::zero() {
            return Err(anyhow!("amount cannot be zero"));
        }

        // lower balance
        BALANCES.update(
            ctx.storage,
            &owner,
            |balance: Option<Uint128>| -> Result<_> {
                Ok(balance.unwrap_or_default().checked_sub(amount)?)
            },
        )?;

        // reduce total_supply
        TOKEN_INFO.update(ctx.storage, |mut info| -> Result<_> {
            info.total_supply = info.total_supply.checked_sub(amount)?;
            Ok(info)
        })?;

        Ok(())
    }

    pub(crate) fn burn_from(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        owner: Addr,
        amount: Uint128,
    ) -> Result<()> {
        ctx.response.add_event(BurnEvent {
            owner: owner.clone(),
            amount,
            by: Some(sender.clone()),
        });

        // deduct allowance before doing anything else have enough allowance
        self.deduct_allowance(ctx, &owner, &sender, amount)?;

        // lower balance
        BALANCES.update(
            ctx.storage,
            &owner,
            |balance: Option<Uint128>| -> Result<_> {
                Ok(balance.unwrap_or_default().checked_sub(amount)?)
            },
        )?;

        // reduce total_supply
        TOKEN_INFO.update(ctx.storage, |mut meta| -> Result<_> {
            meta.total_supply = meta.total_supply.checked_sub(amount)?;
            Ok(meta)
        })?;

        Ok(())
    }

    pub(crate) fn send_with_msg(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        contract: Addr,
        amount: Uint128,
        msg: Binary,
    ) -> Result<()> {
        ctx.response.add_event(SendEvent {
            owner: owner.clone(),
            contract: contract.clone(),
            amount,
            by: None,
        });

        if amount == Uint128::zero() {
            return Err(anyhow!("amount cannot be zero"));
        }

        // move the tokens to the contract
        BALANCES.update(
            ctx.storage,
            &owner,
            |balance: Option<Uint128>| -> Result<_> {
                Ok(balance.unwrap_or_default().checked_sub(amount)?)
            },
        )?;

        BALANCES.update(
            ctx.storage,
            &contract,
            |balance: Option<Uint128>| -> Result<_> { Ok(balance.unwrap_or_default() + amount) },
        )?;

        ctx.response.add_execute_submessage_oneshot(
            contract,
            &ReceiverExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: owner.to_string(),
                amount,
                msg,
            }),
        )?;

        Ok(())
    }
    pub(crate) fn send_with_msg_from(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        owner: Addr,
        contract: Addr,
        amount: Uint128,
        msg: Binary,
    ) -> Result<()> {
        ctx.response.add_event(SendEvent {
            owner: owner.clone(),
            contract: contract.clone(),
            amount,
            by: Some(sender.clone()),
        });

        // deduct allowance before doing anything else have enough allowance
        self.deduct_allowance(ctx, &owner, &sender, amount)?;

        // move the tokens to the contract
        BALANCES.update(
            ctx.storage,
            &owner,
            |balance: Option<Uint128>| -> Result<_> {
                Ok(balance.unwrap_or_default().checked_sub(amount)?)
            },
        )?;
        BALANCES.update(
            ctx.storage,
            &contract,
            |balance: Option<Uint128>| -> Result<_> { Ok(balance.unwrap_or_default() + amount) },
        )?;

        ctx.response.add_execute_submessage_oneshot(
            contract,
            &ReceiverExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: sender.to_string(),
                amount,
                msg,
            }),
        )?;

        Ok(())
    }

    pub(crate) fn mint(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        recipient: Addr,
        amount: Uint128,
    ) -> Result<()> {
        ctx.response.add_event(MintEvent {
            owner: sender.clone(),
            recipient: recipient.clone(),
            amount,
        });

        if amount == Uint128::zero() {
            return Err(anyhow!("amount cannot be zero"));
        }

        let mut config = TOKEN_INFO
            .may_load(ctx.storage)?
            .context("TOKEN_INFO is empty")?;

        let minter = self.minter_addr(ctx.storage)?;
        if minter != sender {
            return Err(anyhow!(
                "Cannot mint, sender is {sender}, minter is {minter}"
            ));
        }

        // update supply and enforce cap
        config.total_supply += amount;

        if let Some(limit) = self.minter_cap(ctx.storage)? {
            if config.total_supply > limit {
                return Err(perp_anyhow!(
                    ErrorId::Cw20Funds,
                    ErrorDomain::Cw20,
                    "amount cannot be zero"
                ));
            }
        }
        TOKEN_INFO.save(ctx.storage, &config)?;

        // add amount to recipient balance
        BALANCES.update(
            ctx.storage,
            &recipient,
            |balance: Option<Uint128>| -> Result<_> { Ok(balance.unwrap_or_default() + amount) },
        )?;

        Ok(())
    }

    pub(crate) fn increase_allowance(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        spender: Addr,
        amount: Uint128,
        expires: Option<Expiration>,
    ) -> Result<()> {
        ctx.response.add_event(AllowanceChangeEvent {
            kind: AllowanceChangeKind::Increase,
            owner: owner.clone(),
            spender: spender.clone(),
            amount,
            expires,
        });

        if spender == owner {
            return Err(perp_anyhow!(
                ErrorId::Auth,
                ErrorDomain::Cw20,
                "cannot increase allowance to own account"
            ));
        }

        let update_fn = |allow: Option<AllowanceResponse>| -> Result<_> {
            let mut val = allow.unwrap_or_default();
            if let Some(exp) = expires {
                val.expires = exp;
            }
            val.allowance += amount;
            Ok(val)
        };
        ALLOWANCES.update(ctx.storage, (&owner, &spender), update_fn)?;
        ALLOWANCES_SPENDER.update(ctx.storage, (&spender, &owner), update_fn)?;

        Ok(())
    }

    pub(crate) fn decrease_allowance(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        spender: Addr,
        amount: Uint128,
        expires: Option<Expiration>,
    ) -> Result<()> {
        ctx.response.add_event(AllowanceChangeEvent {
            kind: AllowanceChangeKind::Decrease,
            owner: owner.clone(),
            spender: spender.clone(),
            amount,
            expires,
        });

        if spender == owner {
            return Err(perp_anyhow!(
                ErrorId::Auth,
                ErrorDomain::Cw20,
                "cannot decrease allowance to own account"
            ));
        }

        let key = (&owner, &spender);

        fn reverse<'a>(t: (&'a Addr, &'a Addr)) -> (&'a Addr, &'a Addr) {
            (t.1, t.0)
        }

        // load value and delete if it hits 0, or update otherwise
        let mut allowance = ALLOWANCES.load(ctx.storage, key)?;
        if amount < allowance.allowance {
            // update the new amount
            allowance.allowance = allowance.allowance.checked_sub(amount)?;

            if let Some(exp) = expires {
                allowance.expires = exp;
            }
            ALLOWANCES.save(ctx.storage, key, &allowance)?;
            ALLOWANCES_SPENDER.save(ctx.storage, reverse(key), &allowance)?;
        } else {
            ALLOWANCES.remove(ctx.storage, key);
            ALLOWANCES_SPENDER.remove(ctx.storage, reverse(key));
        }

        Ok(())
    }

    // this can be used to update a lower allowance - call bucket.update with proper keys
    fn deduct_allowance(
        &self,
        ctx: &mut StateContext,
        owner: &Addr,
        spender: &Addr,
        amount: Uint128,
    ) -> Result<AllowanceResponse> {
        let block = self.env.block.clone();

        let update_fn = |current: Option<AllowanceResponse>| -> _ {
            match current {
                Some(mut a) => {
                    if a.expires.is_expired(&block) {
                        Err(perp_anyhow!(ErrorId::Expired, ErrorDomain::Cw20, ""))
                    } else {
                        // deduct the allowance if enough
                        a.allowance = a.allowance.checked_sub(amount)?;
                        Ok(a)
                    }
                }
                None => Err(perp_anyhow!(
                    ErrorId::Auth,
                    ErrorDomain::Cw20,
                    "no allowance"
                )),
            }
        };
        ALLOWANCES.update(ctx.storage, (owner, spender), update_fn)?;
        ALLOWANCES_SPENDER.update(ctx.storage, (spender, owner), update_fn)
    }

    pub(crate) fn set_marketing(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        project: Option<String>,
        description: Option<String>,
        marketing: Option<String>,
    ) -> Result<()> {
        ctx.response.add_event(MarketingChangeEvent {
            project: project.clone(),
            description: description.clone(),
            marketing: marketing.clone(),
        });

        let mut marketing_info = MARKETING_INFO.load(ctx.storage)?;

        if marketing_info
            .marketing
            .as_ref()
            .ok_or_else(|| perp_anyhow!(ErrorId::Auth, ErrorDomain::Cw20, ""))?
            != sender
        {
            return Err(perp_anyhow!(ErrorId::Auth, ErrorDomain::Cw20, ""));
        }

        match project {
            Some(empty) if empty.trim().is_empty() => marketing_info.project = None,
            Some(project) => marketing_info.project = Some(project),
            None => (),
        }

        match description {
            Some(empty) if empty.trim().is_empty() => marketing_info.description = None,
            Some(description) => marketing_info.description = Some(description),
            None => (),
        }

        match marketing {
            Some(empty) if empty.trim().is_empty() => marketing_info.marketing = None,
            Some(marketing) => marketing_info.marketing = Some(self.api.addr_validate(&marketing)?),
            None => (),
        }

        if marketing_info.project.is_none()
            && marketing_info.description.is_none()
            && marketing_info.marketing.is_none()
            && marketing_info.logo.is_none()
        {
            MARKETING_INFO.remove(ctx.storage);
        } else {
            MARKETING_INFO.save(ctx.storage, &marketing_info)?;
        }

        Ok(())
    }

    pub(crate) fn set_logo(&self, ctx: &mut StateContext, sender: Addr, logo: Logo) -> Result<()> {
        ctx.response.add_event(LogoChangeEvent { logo: &logo });

        let mut marketing_info = MARKETING_INFO.load(ctx.storage)?;

        if marketing_info
            .marketing
            .as_ref()
            .ok_or_else(|| perp_anyhow!(ErrorId::Auth, ErrorDomain::Cw20, ""))?
            != sender
        {
            return Err(perp_anyhow!(ErrorId::Auth, ErrorDomain::Cw20, ""));
        }

        LOGO.save(ctx.storage, &logo)?;

        let logo_info = match logo {
            Logo::Url(url) => LogoInfo::Url(url),
            Logo::Embedded(_) => LogoInfo::Embedded,
        };

        marketing_info.logo = Some(logo_info);
        MARKETING_INFO.save(ctx.storage, &marketing_info)?;

        Ok(())
    }

    pub(crate) fn set_minter(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        new_minter: RawAddr,
    ) -> Result<()> {
        let old_minter_addr = self.minter_addr(ctx.storage)?;

        if old_minter_addr != sender {
            return Err(perp_anyhow!(
                ErrorId::Auth,
                ErrorDomain::Cw20,
                "Not the minter! Sender is {sender}, minter is {old_minter_addr}"
            ));
        }

        let new_minter = new_minter.validate(self.api)?;
        MINTER.save(ctx.storage, &new_minter)?;

        ctx.response
            .add_event(MinterChangeEvent { minter: new_minter });

        Ok(())
    }
}
