use std::path::Path;

use jikji_core::io_error;

pub(crate) fn sqlite_error(path: &Path, source: rusqlite::Error) -> jikji_core::JikjiError {
    io_error(path, std::io::Error::other(source))
}
