use alloc::string::String;

use axerrno::{AxError, AxResult};
use linux_raw_sys::general::AT_EMPTY_PATH;

/// Linux *at syscalls: empty pathname requires `AT_EMPTY_PATH`; otherwise EINVAL (issue-399;
/// issue-401 `stat`/`access`; orthogonality with NULL + `AT_EMPTY_PATH`, issue-331; issue-393 theme).
pub(crate) fn reject_empty_pathname_without_empty_path_flag(
    path: &Option<String>,
    flags: u32,
) -> AxResult<()> {
    if let Some(p) = path
        && p.is_empty()
        && flags & AT_EMPTY_PATH == 0
    {
        return Err(AxError::InvalidInput);
    }
    Ok(())
}
