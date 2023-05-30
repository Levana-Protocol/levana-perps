use cosmwasm_schema::cw_serde;

#[cw_serde]
pub struct DragonMintExtra {
    #[serde(rename = "dragon_id")]
    pub id: String,
    #[serde(rename = "baby_dragon_cid")]
    pub cid: String,
    pub eye_color: String,
    #[serde(rename = "dragon_type")]
    pub kind: String,
}

impl DragonMintExtra {
    pub fn image_ipfs_url(&self) -> String {
        format!("ipfs://{}", self.cid)
    }
}
