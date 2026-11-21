//! A minimal columnar table → Parquet writer and reader on the `parquet` crate's low-level API
//! (no Arrow): just the column types the manifest and the extractor use. Files are one row
//! group, zstd-compressed.

use std::collections::BTreeMap;
use std::sync::Arc;

use parquet::basic::{Compression, LogicalType, Repetition, Type as Physical, ZstdLevel};
use parquet::column::writer::ColumnWriter;
use parquet::data_type::ByteArray;
use parquet::file::properties::WriterProperties;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::file::writer::SerializedFileWriter;
use parquet::record::Field;
use parquet::schema::types::Type;

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

/// A column's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// UTF-8 string.
    Str,
    /// 64-bit integer.
    I64,
    /// 64-bit float.
    F64,
    /// Boolean.
    Bool,
}

/// One column: a name, a type, whether it may be null, and its values.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    /// Name.
    pub name: String,
    /// Type.
    pub kind: Kind,
    /// May hold nulls.
    pub optional: bool,
    /// One value per row.
    pub values: Vec<Value>,
}

impl Column {
    /// An empty column.
    pub fn new(name: impl Into<String>, kind: Kind, optional: bool) -> Self {
        Column {
            name: name.into(),
            kind,
            optional,
            values: Vec::new(),
        }
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn field_type(c: &Column) -> Result<Type, String> {
    let (physical, logical) = match c.kind {
        Kind::Str => (Physical::BYTE_ARRAY, Some(LogicalType::String)),
        Kind::I64 => (Physical::INT64, None),
        Kind::F64 => (Physical::DOUBLE, None),
        Kind::Bool => (Physical::BOOLEAN, None),
    };
    Type::primitive_type_builder(&c.name, physical)
        .with_repetition(if c.optional {
            Repetition::OPTIONAL
        } else {
            Repetition::REQUIRED
        })
        .with_logical_type(logical)
        .build()
        .map_err(err)
}

/// Picks a column's present values out (`pick`) and its definition levels.
fn split<T>(
    c: &Column,
    pick: impl Fn(&Value) -> Option<T>,
) -> Result<(Vec<T>, Option<Vec<i16>>), String> {
    let mut values = Vec::with_capacity(c.values.len());
    let mut levels = Vec::with_capacity(c.values.len());
    for v in &c.values {
        if *v == Value::Null {
            if !c.optional {
                return Err(format!("column {}: null in a required column", c.name));
            }
            levels.push(0);
            continue;
        }
        let x = pick(v).ok_or_else(|| format!("column {}: {v:?} is not {:?}", c.name, c.kind))?;
        values.push(x);
        levels.push(1);
    }
    Ok((values, c.optional.then_some(levels)))
}

/// Writes columns (all the same length) as a Parquet file.
pub fn write(columns: &[Column]) -> Result<Vec<u8>, String> {
    let rows = columns.first().map_or(0, |c| c.values.len());
    if let Some(c) = columns.iter().find(|c| c.values.len() != rows) {
        return Err(format!(
            "column {} has {} rows, not {rows}",
            c.name,
            c.values.len()
        ));
    }
    let fields = columns
        .iter()
        .map(|c| field_type(c).map(Arc::new))
        .collect::<Result<Vec<_>, _>>()?;
    let schema = Type::group_type_builder("schema")
        .with_fields(fields)
        .build()
        .map_err(err)?;
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3).map_err(err)?))
        .set_created_by("opentycoon sim-cli".into())
        .build();
    let mut writer =
        SerializedFileWriter::new(Vec::new(), Arc::new(schema), Arc::new(props)).map_err(err)?;
    let mut group = writer.next_row_group().map_err(err)?;
    for c in columns {
        let Some(mut col) = group.next_column().map_err(err)? else {
            return Err(format!("no writer for column {}", c.name));
        };
        match col.untyped() {
            ColumnWriter::ByteArrayColumnWriter(w) => {
                let (v, d) = split(c, |v| match v {
                    Value::Str(s) => Some(ByteArray::from(s.as_str())),
                    _ => None,
                })?;
                w.write_batch(&v, d.as_deref(), None).map_err(err)?;
            }
            ColumnWriter::Int64ColumnWriter(w) => {
                let (v, d) = split(c, |v| match v {
                    Value::I64(x) => Some(*x),
                    _ => None,
                })?;
                w.write_batch(&v, d.as_deref(), None).map_err(err)?;
            }
            ColumnWriter::DoubleColumnWriter(w) => {
                let (v, d) = split(c, |v| match v {
                    Value::F64(x) => Some(*x),
                    _ => None,
                })?;
                w.write_batch(&v, d.as_deref(), None).map_err(err)?;
            }
            ColumnWriter::BoolColumnWriter(w) => {
                let (v, d) = split(c, |v| match v {
                    Value::Bool(x) => Some(*x),
                    _ => None,
                })?;
                w.write_batch(&v, d.as_deref(), None).map_err(err)?;
            }
            _ => return Err(format!("column {}: unsupported type", c.name)),
        }
        col.close().map_err(err)?;
    }
    group.close().map_err(err)?;
    writer.into_inner().map_err(err)
}

fn reader(bytes: &[u8]) -> Result<SerializedFileReader<bytes::Bytes>, String> {
    SerializedFileReader::new(bytes::Bytes::copy_from_slice(bytes)).map_err(err)
}

/// The column names of a Parquet file, in order.
pub fn columns(bytes: &[u8]) -> Result<Vec<String>, String> {
    let r = reader(bytes)?;
    Ok(r.metadata()
        .file_metadata()
        .schema_descr()
        .columns()
        .iter()
        .map(|c| c.name().to_owned())
        .collect())
}

/// Reads every row of a Parquet file as column name → value.
pub fn read(bytes: &[u8]) -> Result<Vec<BTreeMap<String, Value>>, String> {
    let r = reader(bytes)?;
    let mut out = Vec::new();
    for row in r.get_row_iter(None).map_err(err)? {
        let row = row.map_err(err)?;
        let mut map = BTreeMap::new();
        for (name, field) in row.get_column_iter() {
            let v = match field {
                Field::Null => Value::Null,
                Field::Bool(b) => Value::Bool(*b),
                Field::Long(x) => Value::I64(*x),
                Field::Double(x) => Value::F64(*x),
                Field::Str(s) => Value::Str(s.clone()),
                other => return Err(format!("column {name}: unexpected {other:?}")),
            };
            map.insert(name.clone(), v);
        }
        out.push(map);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_round_trips_with_nulls() {
        let mut a = Column::new("a", Kind::Str, false);
        let mut b = Column::new("b", Kind::I64, true);
        let mut c = Column::new("c", Kind::F64, false);
        let mut d = Column::new("d", Kind::Bool, true);
        for i in 0..3_i64 {
            a.values.push(Value::Str(format!("r{i}")));
            b.values.push(if i == 1 {
                Value::Null
            } else {
                Value::I64(i - 5)
            });
            c.values
                .push(Value::F64([0.0, 0.5, 1.0][usize::try_from(i).unwrap()]));
            d.values.push(if i == 2 {
                Value::Null
            } else {
                Value::Bool(i == 0)
            });
        }
        let bytes = write(&[a, b, c, d]).unwrap();
        assert_eq!(&bytes[0..4], b"PAR1");
        assert_eq!(columns(&bytes).unwrap(), ["a", "b", "c", "d"]);
        let rows = read(&bytes).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1]["a"], Value::Str("r1".into()));
        assert_eq!(rows[1]["b"], Value::Null);
        assert_eq!(rows[2]["b"], Value::I64(-3));
        assert_eq!(rows[2]["c"], Value::F64(1.0));
        assert_eq!(rows[2]["d"], Value::Null);
        assert_eq!(rows[0]["d"], Value::Bool(true));
    }

    #[test]
    fn a_null_in_a_required_column_or_a_wrong_type_is_an_error() {
        let mut a = Column::new("a", Kind::Str, false);
        a.values.push(Value::Null);
        assert!(write(&[a]).unwrap_err().contains("required"));
        let mut b = Column::new("b", Kind::I64, false);
        b.values.push(Value::Str("x".into()));
        assert!(write(&[b]).is_err());
    }
}
