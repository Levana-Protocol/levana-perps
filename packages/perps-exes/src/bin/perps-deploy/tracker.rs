use cosmos::{
    proto::{cosmos::base::abci::v1beta1::TxResponse, cosmwasm::wasm::v1::MsgExecuteContract},
    Address, CodeId, Contract, HasAddress, HasCosmos, TxBuilder, Wallet,
};
use msg::contracts::tracker::entry::{CodeIdResp, ContractResp, ExecuteMsg, QueryMsg};
use msg::prelude::*;

use crate::{cli::Opt, util::get_hash_for_path};

pub(crate) struct Tracker(Contract);

impl Tracker {
    pub(crate) fn from_contract(contract: Contract) -> Self {
        Tracker(contract)
    }

    pub(crate) async fn get_code_by_hash(&self, hash: String) -> Result<CodeIdResp> {
        self.0.query(QueryMsg::CodeByHash { hash }).await
    }

    pub(crate) async fn require_code_by_type(
        &self,
        opt: &Opt,
        contract_type: &str,
    ) -> Result<CodeId> {
        let path = opt.get_contract_path(contract_type);
        let hash = get_hash_for_path(&path)?;
        match self.get_code_by_hash(hash).await? {
            CodeIdResp::NotFound {} => Err(anyhow::anyhow!(
                "Contract at {} is not logged with the tracker, please store code first",
                path.display()
            )),
            CodeIdResp::Found { code_id, .. } => Ok(self.0.get_cosmos().make_code_id(code_id)),
        }
    }

    pub(crate) async fn store_code(
        &self,
        wallet: &Wallet,
        contract_type: String,
        code_id: u64,
        hash: String,
        gitrev: String,
    ) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                ExecuteMsg::CodeId {
                    contract_type,
                    code_id,
                    hash,
                    gitrev: Some(gitrev),
                },
            )
            .await
    }

    pub(crate) async fn instantiate(
        &self,
        wallet: &Wallet,
        to_log: &[(u64, Address)],
        family: impl Into<String>,
    ) -> Result<TxResponse> {
        let mut builder = TxBuilder::default();
        let family = family.into();
        for (code_id, addr) in to_log.iter().copied() {
            builder.add_message_mut(MsgExecuteContract {
                sender: wallet.get_address_string(),
                contract: self.0.get_address_string(),
                msg: serde_json::to_vec(&ExecuteMsg::Instantiate {
                    code_id,
                    address: addr.get_address_string(),
                    family: family.clone(),
                })?,
                funds: vec![],
            });
        }
        builder
            .sign_and_broadcast(self.0.get_cosmos(), wallet)
            .await
    }

    pub(crate) async fn get_contract_by_family(
        &self,
        contract_type: impl Into<String>,
        family: impl Into<String>,
        sequence: Option<u32>,
    ) -> Result<ContractResp> {
        self.0
            .query(QueryMsg::ContractByFamily {
                contract_type: contract_type.into(),
                family: family.into(),
                sequence,
            })
            .await
    }

    pub(crate) async fn migrate(
        &self,
        wallet: &Wallet,
        new_code_id: u64,
        address: impl HasAddress,
    ) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                msg::contracts::tracker::entry::ExecuteMsg::Migrate {
                    new_code_id,
                    address: address.get_address_string(),
                },
            )
            .await
    }
}
