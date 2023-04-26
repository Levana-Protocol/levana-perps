use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};

pub(crate) fn get_hash_for_path(path: &Path) -> Result<String> {
    let mut file = fs_err::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}
