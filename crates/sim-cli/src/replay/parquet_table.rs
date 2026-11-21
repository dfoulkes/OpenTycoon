//! A minimal columnar table → Parquet writer and reader on the `parquet` crate's low-level API
//! (no Arrow): just the column types the manifest and the extractor use.

use std::collections::BTreeMap;

/// One cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Null (an optional column with no value).
    Null,
    /// UTF-8 string.
    Str(String),
    /// 64-bit integer.
    I64(i64),
    /// 64-bit float.
    F64(f64),
    /// Boolean.
    Bool(bool),
}

/// Reads every row of a Parquet file as column name → value.
pub fn read(bytes: &[u8]) -> Result<Vec<BTreeMap<String, Value>>, String> {
    let _ = bytes;
    Err("not implemented".into())
}

/// The column names of a Parquet file, in order.
pub fn columns(bytes: &[u8]) -> Result<Vec<String>, String> {
    let _ = bytes;
    Err("not implemented".into())
}
