// this works in tandem with cli/schema.mjs
//
// so... the raw json entry files are buggy and weird
// however, the schema generator writes the schema data to a single file which has valid schema
// on the inside, not as a whole :/
//
// so, if this top-level file is loaded as JSON, the individual objects can be parsed as valid schema
//
// weird, but not the end of the world
//
// then one more thing - the response files are not in that top-level file
// rather, they need to be copied over from the raw folder
// and hope that unlike the raw entry files, these are not corrupted
//
// so ultimately, it's a bit of song and dance, but we end up writing out:
//
// 1. top-level file which should be loaded and then the inner contents contains schema data
// 2. additional response_to files which are valid schema
//
// and this is then converted to typescript
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use cosmwasm_schema::write_api;
use fs_extra::dir::{move_dir, CopyOptions};
use levana_perpswap_cosmos_msg::prelude::PerpError;
const JSON_FOLDER: &str = "../../schema/json";

struct Paths {
    pub schema: PathBuf,
    pub raw: PathBuf,
    pub output: PathBuf,
}
impl Paths {
    pub fn new() -> Self {
        Self {
            schema: Path::new("./schema").to_path_buf(),
            raw: Path::new("./schema").join("raw"),
            output: Path::new(JSON_FOLDER).to_path_buf(),
        }
    }
}

fn main() {
    let paths = Paths::new();
    fs_extra::dir::remove(&paths.schema).unwrap();
    fs_extra::dir::remove(&paths.output).unwrap();
    create_dir_all(&paths.output).unwrap();

    // yeah, this is super weird. don't ask
    // as of right now, one of the weird things is re-using the same output file
    // so gotta wipe the dir each time
    fn finalize(name: &str) {
        let paths = Paths::new();

        // newest version adds the responses under responses in the top level object
        // instead of separate files. But it's brand-new, and this is backwards-compatible
        // so not deleting quite yet
        if let Ok(dir_content) = fs_extra::dir::get_dir_content(&paths.raw) {
            for file in dir_content.files {
                if file.contains("response_to_") {
                    let src = Path::new(&file);
                    let dest = paths.output.join(format!(
                        "{}_{}",
                        name,
                        src.file_name().unwrap().to_str().unwrap()
                    ));
                    fs_extra::file::move_file(src, dest, &fs_extra::file::CopyOptions::default())
                        .unwrap();
                }
            }
        }
        let _ = fs_extra::dir::remove(&paths.raw);
        move_dir(
            &paths.schema,
            &paths.output,
            &CopyOptions {
                content_only: true,
                ..Default::default()
            },
        )
        .unwrap();

        fs_extra::dir::remove(&paths.schema).unwrap();
    }

    write_cw20();
    finalize("cw20");

    write_factory();
    finalize("factory");

    write_farming();
    finalize("farming");

    write_faucet();
    finalize("faucet");

    write_hatching();
    finalize("hatching");

    write_ibc_execute_proxy();
    finalize("ibc_execute_proxy");

    write_liquidity_token();
    finalize("liquidity_token");

    write_market();
    finalize("market");

    write_position_token();
    finalize("position_token");

    write_error();
    finalize("error");
}

fn write_cw20() {
    use levana_perpswap_cosmos_msg::contracts::cw20::entry::{
        ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    };

    write_api!(
    name: "cw20",
    instantiate: InstantiateMsg,
    execute: ExecuteMsg,
    query: QueryMsg,
    migrate: MigrateMsg,
    );
}

fn write_position_token() {
    use levana_perpswap_cosmos_msg::contracts::position_token::entry::{
        ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    };

    write_api!(
    name: "position_token",
    instantiate: InstantiateMsg,
    execute: ExecuteMsg,
    query: QueryMsg,
    migrate: MigrateMsg,
    );
}

fn write_liquidity_token() {
    use levana_perpswap_cosmos_msg::contracts::liquidity_token::entry::{
        ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    };

    write_api!(
        name: "liquidity_token",
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
        migrate: MigrateMsg,
    );
}

fn write_factory() {
    use levana_perpswap_cosmos_msg::contracts::factory::entry::{
        ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    };

    write_api!(
        name: "factory",
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
        migrate: MigrateMsg,
    );
}

fn write_market() {
    use levana_perpswap_cosmos_msg::contracts::market::entry::{
        ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    };

    write_api!(
    name: "market",
    instantiate: InstantiateMsg,
    execute: ExecuteMsg,
    query: QueryMsg,
    migrate: MigrateMsg,
    );
}

fn write_faucet() {
    use levana_perpswap_cosmos_msg::contracts::faucet::entry::{
        ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    };

    write_api!(
    name: "faucet",
    instantiate: InstantiateMsg,
    execute: ExecuteMsg,
    query: QueryMsg,
    migrate: MigrateMsg,
    );
}

fn write_farming() {
    use levana_perpswap_cosmos_msg::contracts::farming::entry::{
        ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    };

    write_api!(
    name: "farming",
    instantiate: InstantiateMsg,
    execute: ExecuteMsg,
    query: QueryMsg,
    migrate: MigrateMsg,
    );
}

fn write_hatching() {
    use levana_perpswap_cosmos_msg::contracts::hatching::entry::{
        ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    };

    write_api!(
    name: "hatching",
    instantiate: InstantiateMsg,
    execute: ExecuteMsg,
    query: QueryMsg,
    migrate: MigrateMsg,
    );
}

fn write_ibc_execute_proxy() {
    use levana_perpswap_cosmos_msg::contracts::ibc_execute_proxy::entry::{
        ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    };

    write_api!(
    name: "ibc_execute_proxy",
    instantiate: InstantiateMsg,
    execute: ExecuteMsg,
    query: QueryMsg,
    migrate: MigrateMsg,
    );
}

fn write_error() {
    // this doesn't use the `write_api!()` helper
    // since it's not part a contract entry

    let paths = Paths::new();
    create_dir_all(&paths.schema).unwrap();

    let path = paths.schema.join("error.json");
    let obj = cosmwasm_schema::schema_for!(PerpError);
    let json = serde_json::to_string_pretty(&obj).unwrap();
    std::fs::write(path, json).unwrap();
}
