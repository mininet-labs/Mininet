//! `mini beta grant-policy|grant|grant-approve|grant-status`: decentralised,
//! threshold-approved Beta MINI grants (`mini-beta-grants`). Test value only.

use std::path::Path;

use crate::error::{CliError, Result};
use crate::json::CommandResult;

/// Route `mini beta <noun> <sub>`; returns the JSON `kind` and the outcome.
pub fn dispatch(
    _home: &Path,
    _store: &Path,
    noun: &str,
    sub: &str,
    _args: Vec<String>,
) -> (&'static str, Result<CommandResult>) {
    (
        "beta.unimplemented",
        Err(CliError::Usage(format!(
            "unknown `beta` command: {noun:?} {sub:?}"
        ))),
    )
}
