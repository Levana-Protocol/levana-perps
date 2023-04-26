/*
 * This is essentially a copy/paste/adapt from the reference cw721 spec
 * there are a few overall concepts:
 *
 * 1. Execute messages *always* get sent from the proxy contract
 * 2. Query messages can come from anywhere
 * 3. Since the owner is often used in business logic, it is stored directly in Position
 * 4. Everything else which is only needed to satisfy the CW721 spec is stored in this module
 * 5. We care much less about gas costs for proxied calls than core market operations
 * 6. Only expose mutable functions that are genuinely necessary for the other market modules
 */

use crate::prelude::*;
use cosmwasm_std::{Binary, BlockInfo, Order, QueryResponse};
use cw_utils::Expiration;
use msg::contracts::position_token::{
    entry::{
        AllNftInfoResponse, ApprovalResponse, ApprovalsResponse, ExecuteMsg, NftContractInfo,
        NftInfoResponse, NumTokensResponse, OperatorsResponse, OwnerOfResponse, QueryMsg,
        TokensResponse,
    },
    events::{
        ApprovalEvent, ApproveAllEvent, BurnEvent, MintEvent, RevokeAllEvent, RevokeEvent,
        TransferEvent,
    },
    Approval, Cw721ReceiveMsg, FullTokenInfo, Metadata, Trait,
};

use super::get_position;

// this is unbounded, but that's how it is in the cw721 reference too
// https://github.com/CosmWasm/cw-nfts/blob/bf70cfb516b39a49db423a4b353c2bb8518c2b51/contracts/cw721-base/src/state.rs#L108
const APPROVALS: Map<String, Vec<Approval>> = Map::new(namespace::NFT_APPROVALS);
const OPERATORS: Map<(&Addr, &Addr), Expiration> = Map::new(namespace::NFT_OPERATORS);
const TOKEN_COUNT: Item<u64> = Item::new(namespace::NFT_COUNT);
const OWNER_TO_TOKEN_IDS: Map<(&Addr, String), u8> = Map::new(namespace::NFT_OWNERS);
const TOKEN_IDS: Map<String, u8> = Map::new(namespace::NFT_POSITION_IDS);
const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 100;

impl State<'_> {
    pub(crate) fn nft_handle_query(
        &self,
        store: &dyn Storage,
        msg: QueryMsg,
    ) -> Result<QueryResponse> {
        match msg {
            // this is the one query which is actually handled in the proxy itself
            // so redirect
            QueryMsg::Version {} => {
                let position_token_addr = self.position_token_addr(store)?;
                self.querier
                    .query_wasm_smart(position_token_addr, &msg)
                    .map_err(|err| err.into())
            }
            QueryMsg::ContractInfo {} => self.nft_contract_info(store)?.query_result(),

            QueryMsg::NftInfo { token_id } => self.nft_info(store, token_id)?.query_result(),

            QueryMsg::OwnerOf {
                token_id,
                include_expired,
            } => self
                .nft_owner_of(store, token_id, include_expired.unwrap_or(false))?
                .query_result(),

            QueryMsg::AllNftInfo {
                token_id,
                include_expired,
            } => self
                .nft_all_info(store, token_id, include_expired.unwrap_or(false))?
                .query_result(),

            QueryMsg::AllOperators {
                owner,
                include_expired,
                start_after,
                limit,
            } => self
                .nft_operators(
                    store,
                    owner.validate(self.api)?,
                    include_expired.unwrap_or(false),
                    cw_utils::maybe_addr(self.api, start_after)?,
                    limit,
                )?
                .query_result(),

            QueryMsg::NumTokens {} => self.nft_num_tokens(store)?.query_result(),

            QueryMsg::Tokens {
                owner,
                start_after,
                limit,
            } => {
                let owner = owner.validate(self.api)?;

                TokensResponse {
                    tokens: self.nft_map_token_ids(
                        store,
                        Some(&owner),
                        start_after,
                        limit,
                        |id| id,
                    )?,
                }
                .query_result()
            }

            QueryMsg::AllTokens { start_after, limit } => TokensResponse {
                tokens: self.nft_map_token_ids(store, None, start_after, limit, |id| id)?,
            }
            .query_result(),

            QueryMsg::Approval {
                token_id,
                spender,
                include_expired,
            } => self
                .nft_approval(
                    store,
                    token_id,
                    spender.validate(self.api)?,
                    include_expired.unwrap_or(false),
                )?
                .query_result(),

            QueryMsg::Approvals {
                token_id,
                include_expired,
            } => self
                .nft_approvals(store, token_id, include_expired.unwrap_or(false))?
                .query_result(),
        }
    }

    fn nft_token_full(&self, store: &dyn Storage, token_id: String) -> Result<FullTokenInfo> {
        let market_id = self.market_id(store)?;
        let position_id = PositionId(token_id.parse()?);
        let approvals = self.nft_token_approvals(store, token_id)?;
        let position = get_position(store, position_id)?;

        Ok(mixin_token(market_id, approvals, position))
    }

    fn nft_map_token_ids<A, F>(
        &self,
        store: &dyn Storage,
        owner: Option<&Addr>,
        start_after: Option<String>,
        limit: Option<u32>,
        f: F,
    ) -> Result<Vec<A>>
    where
        F: Fn(String) -> A + Clone,
    {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

        let min = start_after.map(Bound::exclusive);

        match owner {
            Some(owner) => {
                let vec = OWNER_TO_TOKEN_IDS
                    .prefix(owner)
                    .keys(store, min, None, cosmwasm_std::Order::Ascending)
                    .take(limit)
                    .map(|res| res.map(f.clone()).map_err(|err| err.into()))
                    .collect::<Result<Vec<A>>>()?;

                Ok(vec)
            }
            None => {
                let vec = TOKEN_IDS
                    .keys(store, min, None, cosmwasm_std::Order::Ascending)
                    .take(limit)
                    .map(|res| res.map(f.clone()).map_err(|err| err.into()))
                    .collect::<Result<Vec<A>>>()?;

                Ok(vec)
            }
        }
    }

    fn nft_token_approvals(&self, store: &dyn Storage, token_id: String) -> Result<Vec<Approval>> {
        APPROVALS.load(store, token_id).map_err(|err| err.into())
    }

    fn nft_contract_info(&self, store: &dyn Storage) -> Result<NftContractInfo> {
        let market_id = self.market_id(store)?;

        Ok(NftContractInfo {
            name: market_id.to_string(),
            symbol: market_id.to_string(),
        })
    }

    fn nft_info(&self, store: &dyn Storage, token_id: String) -> Result<NftInfoResponse> {
        let token = self.nft_token_full(store, token_id)?;

        Ok(NftInfoResponse {
            extension: token.extension,
        })
    }

    fn nft_all_info(
        &self,
        store: &dyn Storage,
        token_id: String,
        include_expired: bool,
    ) -> Result<AllNftInfoResponse> {
        let token = self.nft_token_full(store, token_id)?;

        Ok(AllNftInfoResponse {
            access: OwnerOfResponse {
                owner: token.owner,
                approvals: filter_approvals(&self.env.block, &token.approvals, include_expired),
            },
            info: NftInfoResponse {
                extension: token.extension,
            },
        })
    }

    fn nft_owner_of(
        &self,
        store: &dyn Storage,
        token_id: String,
        include_expired: bool,
    ) -> Result<OwnerOfResponse> {
        let position = get_position(store, PositionId(token_id.parse()?))?;
        let approvals = self.nft_token_approvals(store, token_id)?;

        Ok(OwnerOfResponse {
            owner: position.owner,
            approvals: filter_approvals(&self.env.block, &approvals, include_expired),
        })
    }

    fn nft_operators(
        &self,
        store: &dyn Storage,
        owner: Addr,
        include_expired: bool,
        start_addr: Option<Addr>,
        limit: Option<u32>,
    ) -> Result<OperatorsResponse> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start = start_addr.as_ref().map(Bound::exclusive);

        let res: Result<Vec<_>> = OPERATORS
            .prefix(&owner)
            .range(store, start, None, Order::Ascending)
            .filter(|r| {
                // same unwrap in reference contract: https://github.com/CosmWasm/cw-nfts/blob/bf70cfb516b39a49db423a4b353c2bb8518c2b51/contracts/cw721-base/src/query.rs#L80
                include_expired || r.is_err() || !r.as_ref().unwrap().1.is_expired(&self.env.block)
            })
            .take(limit)
            .map(|res| {
                let (spender, expires) = res?;
                Ok(Approval { spender, expires })
            })
            .collect();
        Ok(OperatorsResponse { operators: res? })
    }

    fn nft_num_tokens(&self, store: &dyn Storage) -> Result<NumTokensResponse> {
        let count = TOKEN_COUNT.may_load(store)?.unwrap_or_default();

        Ok(NumTokensResponse { count })
    }

    fn nft_approval(
        &self,
        store: &dyn Storage,
        token_id: String,
        spender: Addr,
        include_expired: bool,
    ) -> Result<ApprovalResponse> {
        let owner = get_position(store, PositionId(token_id.parse()?))?.owner;
        let approvals = self.nft_token_approvals(store, token_id)?;

        // token owner has absolute approval

        if owner == spender {
            let approval = Approval {
                spender: owner,
                expires: Expiration::Never {},
            };
            return Ok(ApprovalResponse { approval });
        }

        let filtered: Vec<_> = approvals
            .into_iter()
            .filter(|t| t.spender == spender)
            .filter(|t| include_expired || !t.is_expired(&self.env.block))
            .collect();

        if filtered.is_empty() {
            return Err(perp_anyhow!(
                ErrorId::Auth,
                ErrorDomain::PositionToken,
                "approval not found"
            ));
        }
        // we expect only one item
        let approval = filtered[0].clone();

        Ok(ApprovalResponse { approval })
    }

    /// approvals returns all approvals owner given access to
    fn nft_approvals(
        &self,
        store: &dyn Storage,
        token_id: String,
        include_expired: bool,
    ) -> Result<ApprovalsResponse> {
        let approvals = self.nft_token_approvals(store, token_id)?;

        let approvals: Vec<_> = approvals
            .into_iter()
            .filter(|t| include_expired || !t.is_expired(&self.env.block))
            .collect();

        Ok(ApprovalsResponse { approvals })
    }

    pub(crate) fn nft_handle_exec(
        &self,
        ctx: &mut StateContext,
        msg_sender: Addr,
        msg: ExecuteMsg,
    ) -> Result<()> {
        match msg {
            ExecuteMsg::Approve {
                spender,
                token_id,
                expires,
            } => self.nft_approve(
                ctx,
                msg_sender,
                spender.validate(self.api)?,
                token_id,
                expires,
            ),
            ExecuteMsg::Revoke { spender, token_id } => {
                self.nft_revoke(ctx, msg_sender, spender.validate(self.api)?, token_id)
            }

            ExecuteMsg::ApproveAll { operator, expires } => {
                self.nft_approve_all(ctx, msg_sender, operator.validate(self.api)?, expires)
            }

            ExecuteMsg::RevokeAll { operator } => {
                self.nft_revoke_all(ctx, msg_sender, operator.validate(self.api)?)
            }

            ExecuteMsg::TransferNft {
                recipient,
                token_id,
            } => self.nft_transfer(ctx, msg_sender, recipient.validate(self.api)?, token_id),

            ExecuteMsg::SendNft {
                contract,
                token_id,
                msg,
            } => self.nft_send(ctx, msg_sender, contract.validate(self.api)?, token_id, msg),
        }
    }

    pub(crate) fn nft_mint(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        token_id: String,
    ) -> Result<()> {
        TOKEN_IDS.save(ctx.storage, token_id.clone(), &1)?;
        OWNER_TO_TOKEN_IDS.save(ctx.storage, (&owner, token_id.clone()), &1)?;
        APPROVALS.save(ctx.storage, token_id.clone(), &Vec::new())?;

        self.nft_increment_tokens(ctx)?;

        ctx.response_mut().add_event(MintEvent { owner, token_id });

        Ok(())
    }

    pub(crate) fn nft_burn(
        &self,
        ctx: &mut StateContext,
        owner: &Addr,
        token_id: String,
    ) -> Result<()> {
        TOKEN_IDS.remove(ctx.storage, token_id.clone());
        OWNER_TO_TOKEN_IDS.remove(ctx.storage, (owner, token_id.clone()));
        APPROVALS.remove(ctx.storage, token_id.clone());

        self.nft_decrement_tokens(ctx)?;

        ctx.response_mut().add_event(BurnEvent { token_id });

        Ok(())
    }

    pub(crate) fn nft_transfer(
        &self,
        ctx: &mut StateContext,
        msg_sender: Addr,
        recipient: Addr,
        token_id: String,
    ) -> Result<()> {
        self.nft_transfer_inner(ctx, &msg_sender, recipient, token_id)?;

        Ok(())
    }

    pub(crate) fn nft_approve(
        &self,
        ctx: &mut StateContext,
        msg_sender: Addr,
        spender: Addr,
        token_id: String,
        expires: Option<Expiration>,
    ) -> Result<()> {
        self.nft_update_approvals(ctx, msg_sender, spender, token_id, true, expires)?;
        Ok(())
    }

    pub(crate) fn nft_revoke(
        &self,
        ctx: &mut StateContext,
        msg_sender: Addr,
        spender: Addr,
        token_id: String,
    ) -> Result<()> {
        self.nft_update_approvals(ctx, msg_sender, spender, token_id, false, None)?;
        Ok(())
    }

    pub(crate) fn nft_approve_all(
        &self,
        ctx: &mut StateContext,
        msg_sender: Addr,
        operator: Addr,
        expires: Option<Expiration>,
    ) -> Result<()> {
        // reject expired data as invalid
        let expires = expires.unwrap_or_default();
        if expires.is_expired(&self.env.block) {
            return Err(perp_anyhow!(
                ErrorId::Expired,
                ErrorDomain::PositionToken,
                ""
            ));
        }

        // set the operator for us
        OPERATORS.save(ctx.storage, (&msg_sender, &operator), &expires)?;

        ctx.response_mut()
            .add_event(ApproveAllEvent { operator, expires });

        Ok(())
    }

    fn nft_revoke_all(
        &self,
        ctx: &mut StateContext,
        msg_sender: Addr,
        operator: Addr,
    ) -> Result<()> {
        OPERATORS.remove(ctx.storage, (&msg_sender, &operator));

        ctx.response_mut().add_event(RevokeAllEvent { operator });

        Ok(())
    }

    fn nft_update_approvals(
        &self,
        ctx: &mut StateContext,
        msg_sender: Addr,
        spender: Addr,
        token_id: String,
        // if add == false, remove. if add == true, remove then set with this expiration
        add: bool,
        expires: Option<Expiration>,
    ) -> Result<Vec<Approval>> {
        let mut approvals = self.nft_token_approvals(ctx.storage, token_id.clone())?;
        // ensure we have permissions
        self.nft_check_can_approve(ctx.storage, &msg_sender, &token_id)?;

        // update the approval list (remove any for the same spender before adding)
        approvals.retain(|apr| apr.spender != spender);

        // only difference between approve and revoke
        if add {
            // reject expired data as invalid
            let expires = expires.unwrap_or_default();
            if expires.is_expired(&self.env.block) {
                return Err(perp_anyhow!(
                    ErrorId::Expired,
                    ErrorDomain::PositionToken,
                    ""
                ));
            }
            let approval = Approval {
                spender: spender.clone(),
                expires,
            };
            approvals.push(approval);

            ctx.response_mut().add_event(ApprovalEvent {
                spender,
                token_id: token_id.clone(),
                expires,
            });
        } else {
            ctx.response_mut().add_event(RevokeEvent {
                spender,
                token_id: token_id.clone(),
            });
        }

        APPROVALS.save(ctx.storage, token_id, &approvals)?;

        Ok(approvals)
    }

    fn nft_check_can_approve(
        &self,
        store: &dyn Storage,
        msg_sender: &Addr,
        token_id: &str,
    ) -> Result<()> {
        let owner = get_position(store, PositionId(token_id.parse()?))?.owner;
        // owner can approve
        if owner == *msg_sender {
            return Ok(());
        }
        // operator can approve
        let op = OPERATORS.may_load(store, (&owner, msg_sender))?;
        match op {
            Some(ex) => {
                if ex.is_expired(&self.env.block) {
                    Err(perp_anyhow!(ErrorId::Auth, ErrorDomain::PositionToken, ""))
                } else {
                    Ok(())
                }
            }
            None => Err(perp_anyhow!(ErrorId::Auth, ErrorDomain::PositionToken, "")),
        }
    }

    fn nft_send(
        &self,
        ctx: &mut StateContext,
        msg_sender: Addr,
        contract: Addr,
        token_id: String,
        msg: Binary,
    ) -> Result<()> {
        // Transfer token
        self.nft_transfer_inner(ctx, &msg_sender, contract.clone(), token_id.clone())?;

        let msg = Cw721ReceiveMsg {
            sender: msg_sender.to_string(),
            token_id,
            msg,
        };

        ctx.response_mut()
            .add_execute_submessage_oneshot(contract, &msg)?;

        Ok(())
    }

    fn nft_transfer_inner(
        &self,
        ctx: &mut StateContext,
        msg_sender: &Addr,
        recipient: Addr,
        token_id: String,
    ) -> Result<()> {
        let approvals = self.nft_token_approvals(ctx.storage, token_id.clone())?;

        // ensure we have permissions
        self.nft_check_can_send(ctx.storage, msg_sender, &token_id, &approvals)?;

        let mut pos = get_position(ctx.storage, PositionId(token_id.parse()?))?;
        // remove old position.owner
        OWNER_TO_TOKEN_IDS.remove(ctx.storage, (&pos.owner, token_id.clone()));
        // add to new position owner
        OWNER_TO_TOKEN_IDS.save(ctx.storage, (&recipient, token_id.clone()), &1)?;

        let old_owner = std::mem::replace(&mut pos.owner, recipient.clone());
        self.position_save_no_recalc(ctx, &pos)?;

        self.position_history_add_transfer(ctx, &pos, old_owner)?;

        //reset existing approvals
        APPROVALS.save(ctx.storage, token_id.clone(), &Vec::new())?;

        ctx.response_mut().add_event(TransferEvent {
            recipient,
            token_id,
        });

        Ok(())
    }

    /// returns true iff the sender can transfer ownership of the token
    fn nft_check_can_send(
        &self,
        store: &dyn Storage,
        msg_sender: &Addr,
        token_id: &str,
        approvals: &[Approval],
    ) -> Result<()> {
        let owner = get_position(store, PositionId(token_id.parse()?))?.owner;
        // owner and market contract can always send
        let market_addr = self.env.contract.address.clone();
        if owner == *msg_sender || market_addr == *msg_sender {
            return Ok(());
        }

        // any non-expired token approval can send
        if approvals
            .iter()
            .any(|apr| apr.spender == *msg_sender && !apr.is_expired(&self.env.block))
        {
            return Ok(());
        }

        // operator can send
        let op = OPERATORS.may_load(store, (&owner, msg_sender))?;
        match op {
            Some(ex) => {
                if ex.is_expired(&self.env.block) {
                    Err(perp_anyhow!(ErrorId::Auth, ErrorDomain::PositionToken, ""))
                } else {
                    Ok(())
                }
            }
            None => Err(perp_anyhow!(ErrorId::Auth, ErrorDomain::PositionToken, "")),
        }
    }
    fn nft_increment_tokens(&self, ctx: &mut StateContext) -> Result<u64> {
        let val = self.nft_num_tokens(ctx.storage)?.count + 1;
        TOKEN_COUNT.save(ctx.storage, &val)?;
        Ok(val)
    }
    fn nft_decrement_tokens(&self, ctx: &mut StateContext) -> Result<u64> {
        let val = self.nft_num_tokens(ctx.storage)?.count - 1;
        TOKEN_COUNT.save(ctx.storage, &val)?;
        Ok(val)
    }
}

fn mixin_token(
    market_id: &MarketId,
    approvals: Vec<Approval>,
    position: Position,
) -> FullTokenInfo {
    fn attr<A: Into<String>, B: Into<String>>(key: A, value: B) -> Trait {
        Trait {
            display_type: None,
            trait_type: key.into(),
            value: value.into(),
        }
    }

    let attributes = position
        .attributes()
        .into_iter()
        .map(|(key, value)| attr(key, value))
        .chain(vec![attr("market-id", market_id.to_string())])
        .collect();

    FullTokenInfo {
        owner: position.owner,
        approvals,
        extension: Metadata {
            attributes: Some(attributes),
            ..Metadata::default()
        },
    }
}

fn filter_approvals(
    block: &BlockInfo,
    approvals: &[Approval],
    include_expired: bool,
) -> Vec<Approval> {
    approvals
        .iter()
        .filter(|apr| include_expired || !apr.is_expired(block))
        .cloned()
        .collect()
}
