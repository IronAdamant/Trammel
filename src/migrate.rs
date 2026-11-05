//! Read-only SQLite plus WAL importer. The old `trammel.db` is never written.

use crate::json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub fn import_database(path: &Path) -> Result<Value, String> {
    let mut bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if bytes.len() < 100 || &bytes[..16] != b"SQLite format 3\0" {
        return Err("not a SQLite database".into());
    }
    apply_wal(path, &mut bytes);
    let page_size = page_size_of(&bytes)?;
    let reserved = bytes[20] as usize;
    let master = table_rows(&bytes, page_size, reserved, 1)?;
    let mut tables: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (_, row) in &master {
        let typ = cell_text(row.first()).unwrap_or("");
        let name = cell_text(row.get(1)).unwrap_or("");
        if typ != "table" || name.starts_with("sqlite_") { continue; }
        let Some(root) = cell_int(row.get(3)) else { continue };
        if root < 1 { continue; }
        let sql = row.get(4).and_then(|cell| cell_text(Some(cell))).unwrap_or("");
        let columns = column_names(sql);
        let rowid_column = rowid_alias(sql);
        let mut rows = table_rows(&bytes, page_size, reserved, root as usize)?;
        rows.sort_by_key(|(rowid, _)| *rowid);
        let mut records = Vec::new();
        for (rowid, record) in rows {
            let mut object = Vec::new();
            for (index, cell) in record.iter().enumerate() {
                let key = columns.get(index).cloned().unwrap_or_else(|| format!("c{index}"));
                // SQLite keeps an INTEGER PRIMARY KEY in the rowid and stores NULL in the record.
                let value = if Some(index) == rowid_column && matches!(cell, Cell::Null) { Value::Int(rowid) } else { cell_value(cell) };
                object.push((key, value));
            }
            records.push(Value::Object(object));
        }
        tables.insert(name.to_string(), records);
    }
    let mut pairs = vec![("schema_version".to_string(), Value::Int(1))];
    for (name, rows) in tables {
        pairs.push((name, Value::Array(rows)));
    }
    Ok(Value::Object(pairs))
}

pub fn row_count(doc: &Value, table: &str) -> usize {
    crate::canon::get(doc, table).and_then(crate::canon::as_array).map(|r| r.len()).unwrap_or(0)
}

fn cell_text(cell: Option<&Cell>) -> Option<&str> { match cell { Some(Cell::Text(s)) => Some(s), _ => None } }
fn cell_int(cell: Option<&Cell>) -> Option<i64> { match cell { Some(Cell::Int(n)) => Some(*n), Some(Cell::Float(n)) => Some(*n as i64), _ => None } }
fn cell_value(cell: &Cell) -> Value {
    match cell {
        Cell::Null => Value::Null,
        Cell::Int(n) => Value::Int(*n),
        Cell::Float(n) => Value::Float(*n),
        Cell::Text(s) => Value::String(s.clone()),
        Cell::Blob => Value::Null,
    }
}

fn page_size_of(bytes: &[u8]) -> Result<usize, String> {
    let raw = u16::from_be_bytes([bytes[16], bytes[17]]);
    let size = if raw == 1 { 65536 } else { raw as usize };
    if size < 512 || !size.is_power_of_two() { return Err(format!("bad page size {size}")); }
    Ok(size)
}

#[derive(Clone, Debug)]
enum Cell { Null, Int(i64), Float(f64), Text(String), Blob }

fn table_rows(bytes: &[u8], page_size: usize, reserved: usize, root: usize) -> Result<Vec<(i64, Vec<Cell>)>, String> {
    let mut rows = Vec::new();
    let mut stack = vec![root];
    let mut guard = 0;
    while let Some(page_no) = stack.pop() {
        guard += 1;
        if guard > 100_000 { return Err("sqlite page walk exceeded the safety limit".into()); }
        let page = page_bytes(bytes, page_size, page_no)?;
        let header_at = if page_no == 1 { 100 } else { 0 };
        if header_at >= page.len() { continue; }
        let kind = page[header_at];
        let cell_count = u16::from_be_bytes([page[header_at + 3], page[header_at + 4]]) as usize;
        let interior = kind == 0x05 || kind == 0x02;
        let ptr_at = header_at + if interior { 12 } else { 8 };
        if interior {
            let right = u32::from_be_bytes([page[header_at + 8], page[header_at + 9], page[header_at + 10], page[header_at + 11]]) as usize;
            if right > 0 { stack.push(right); }
        }
        if kind != 0x0d && kind != 0x05 { continue; }
        for n in 0..cell_count {
            let at = ptr_at + n * 2;
            if at + 2 > page.len() { break; }
            let cell_off = u16::from_be_bytes([page[at], page[at + 1]]) as usize;
            if interior {
                if let Some(child) = interior_child(page, cell_off) { stack.push(child); }
            } else if let Some((rowid, payload)) = leaf_payload(bytes, page, page_size, reserved, cell_off) {
                rows.push((rowid, decode_record(&payload)));
            }
        }
    }
    Ok(rows)
}

fn page_bytes(bytes: &[u8], page_size: usize, page_no: usize) -> Result<&[u8], String> {
    if page_no == 0 { return Err("sqlite page 0".into()); }
    let start = (page_no - 1) * page_size;
    let end = start + page_size;
    if end > bytes.len() { return Err(format!("sqlite page {page_no} is past the end of the file")); }
    Ok(&bytes[start..end])
}

fn interior_child(page: &[u8], off: usize) -> Option<usize> {
    if off + 4 > page.len() { return None; }
    Some(u32::from_be_bytes([page[off], page[off + 1], page[off + 2], page[off + 3]]) as usize)
}

fn leaf_payload(file: &[u8], page: &[u8], page_size: usize, reserved: usize, off: usize) -> Option<(i64, Vec<u8>)> {
    let (payload_size, next) = read_varint(page, off)?;
    let (rowid, next) = read_varint(page, next)?;
    let payload_size = payload_size as usize;
    let local = local_payload(page_size, reserved, payload_size);
    if next + local > page.len() { return None; }
    let mut payload = page[next..next + local].to_vec();
    if payload.len() < payload_size {
        if next + local + 4 > page.len() { return None; }
        let mut overflow = u32::from_be_bytes([page[next + local], page[next + local + 1], page[next + local + 2], page[next + local + 3]]) as usize;
        let usable = page_size.saturating_sub(reserved);
        while payload.len() < payload_size && overflow > 0 {
            let ov = page_bytes(file, page_size, overflow).ok()?;
            if ov.len() < 4 { break; }
            let take = (payload_size - payload.len()).min(usable.saturating_sub(4));
            payload.extend_from_slice(&ov[4..4 + take]);
            overflow = u32::from_be_bytes([ov[0], ov[1], ov[2], ov[3]]) as usize;
        }
    }
    payload.truncate(payload_size);
    Some((rowid as i64, payload))
}

fn local_payload(page_size: usize, reserved: usize, payload: usize) -> usize {
    let usable = page_size.saturating_sub(reserved).max(1);
    let max_local = usable.saturating_sub(35);
    if payload <= max_local { return payload; }
    let min_local = ((usable.saturating_sub(12)) * 32 / 255).saturating_sub(23);
    let mut local = min_local + ((payload - min_local) % (usable.saturating_sub(4).max(1)));
    if local > max_local { local = min_local; }
    local.min(payload)
}

fn decode_record(payload: &[u8]) -> Vec<Cell> {
    let Some((header_size, header_start)) = read_varint(payload, 0) else { return Vec::new() };
    let header_size = header_size as usize;
    if header_size > payload.len() || header_start > header_size { return Vec::new(); }
    let mut types = Vec::new();
    let mut i = header_start;
    while i < header_size {
        let Some((serial, next)) = read_varint(payload, i) else { break };
        types.push(serial);
        i = next;
    }
    let mut body = header_size;
    let mut cells = Vec::new();
    for serial in types {
        let (cell, used) = read_serial(payload, body, serial);
        cells.push(cell);
        body += used;
    }
    cells
}

fn read_serial(payload: &[u8], at: usize, serial: u64) -> (Cell, usize) {
    let take = |n: usize| payload.get(at..at + n);
    match serial {
        0 => (Cell::Null, 0),
        1 => (Cell::Int(take(1).map(|b| b[0] as i8 as i64).unwrap_or(0)), 1),
        2 => (Cell::Int(take(2).map(|b| i16::from_be_bytes([b[0], b[1]]) as i64).unwrap_or(0)), 2),
        3 => {
            let b = take(3).unwrap_or(&[0, 0, 0]);
            let mut raw = [0u8; 4];
            raw[1..].copy_from_slice(&b[..3.min(b.len())]);
            let n = i32::from_be_bytes(raw) >> 8;
            (Cell::Int(n as i64), 3)
        }
        4 => (Cell::Int(take(4).map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64).unwrap_or(0)), 4),
        5 => {
            let b = take(6).unwrap_or(&[0; 6]);
            let mut raw = [0u8; 8];
            if b.len() == 6 { raw[2..].copy_from_slice(b); }
            (Cell::Int(i64::from_be_bytes(raw) >> 16), 6)
        }
        6 => (Cell::Int(take(8).map(|b| i64::from_be_bytes(b.try_into().unwrap_or([0; 8]))).unwrap_or(0)), 8),
        7 => (Cell::Float(take(8).map(|b| f64::from_be_bytes(b.try_into().unwrap_or([0; 8]))).unwrap_or(0.0)), 8),
        8 => (Cell::Int(0), 0),
        9 => (Cell::Int(1), 0),
        n if n >= 12 && n % 2 == 0 => (Cell::Blob, ((n - 12) / 2) as usize),
        n if n >= 13 && n % 2 == 1 => {
            let len = ((n - 13) / 2) as usize;
            let text = payload.get(at..at + len).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default();
            (Cell::Text(text), len)
        }
        _ => (Cell::Null, 0),
    }
}

fn read_varint(data: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for n in 0..9 {
        let b = *data.get(i)?;
        i += 1;
        if n == 8 { return Some(((value << 8) | b as u64, i)); }
        value = (value << 7) | (b & 0x7f) as u64;
        if b & 0x80 == 0 { return Some((value, i)); }
    }
    None
}

/// Index of the column declared `INTEGER PRIMARY KEY`, the rowid alias.
fn rowid_alias(sql: &str) -> Option<usize> {
    let (start, end) = (sql.find('(')?, sql.rfind(')')?);
    sql[start + 1..end]
        .split(',')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|lower| !(lower.is_empty() || lower.starts_with("primary ") || lower.starts_with("unique ") || lower.starts_with("constraint ") || lower.starts_with("foreign ")))
        .position(|lower| lower.split_whitespace().collect::<Vec<_>>().windows(3).any(|w| w == ["integer", "primary", "key"]))
}

fn column_names(sql: &str) -> Vec<String> {
    let Some(start) = sql.find('(') else { return Vec::new() };
    let Some(end) = sql.rfind(')') else { return Vec::new() };
    let mut names = Vec::new();
    for part in sql[start + 1..end].split(',') {
        let part = part.trim();
        let lower = part.to_ascii_lowercase();
        if part.is_empty() || lower.starts_with("primary ") || lower.starts_with("unique ") || lower.starts_with("constraint ") || lower.starts_with("foreign ") { continue; }
        let name = part.split_whitespace().next().unwrap_or("").trim_matches(|c| c == '"' || c == '\'' || c == '`');
        if !name.is_empty() { names.push(name.to_string()); }
    }
    names
}

fn apply_wal(db_path: &Path, bytes: &mut Vec<u8>) {
    let mut name = db_path.as_os_str().to_os_string();
    name.push("-wal");
    let Ok(wal) = fs::read(std::path::PathBuf::from(name)) else { return };
    if wal.len() < 32 { return; }
    let magic = u32::from_be_bytes([wal[0], wal[1], wal[2], wal[3]]);
    if magic != 0x377f_0682 && magic != 0x377f_0683 { return; }
    let page_size = u32::from_be_bytes([wal[8], wal[9], wal[10], wal[11]]) as usize;
    if page_size < 512 { return; }
    let salt1 = u32::from_be_bytes([wal[16], wal[17], wal[18], wal[19]]);
    let salt2 = u32::from_be_bytes([wal[20], wal[21], wal[22], wal[23]]);
    let frame = 24 + page_size;
    let mut latest = BTreeMap::new();
    let mut committed = BTreeMap::new();
    let mut off = 32;
    while off + frame <= wal.len() {
        let page_no = u32::from_be_bytes([wal[off], wal[off + 1], wal[off + 2], wal[off + 3]]) as usize;
        let db_size = u32::from_be_bytes([wal[off + 4], wal[off + 5], wal[off + 6], wal[off + 7]]);
        let s1 = u32::from_be_bytes([wal[off + 8], wal[off + 9], wal[off + 10], wal[off + 11]]);
        let s2 = u32::from_be_bytes([wal[off + 12], wal[off + 13], wal[off + 14], wal[off + 15]]);
        if s1 != salt1 || s2 != salt2 || page_no == 0 { break; }
        latest.insert(page_no, wal[off + 24..off + 24 + page_size].to_vec());
        if db_size > 0 {
            let need = db_size as usize * page_size;
            if bytes.len() < need { bytes.resize(need, 0); }
            committed.clone_from(&latest);
        }
        off += frame;
    }
    for (page_no, page) in committed {
        let start = (page_no - 1) * page_size;
        let end = start + page_size;
        if end <= bytes.len() && page.len() == page_size { bytes[start..end].copy_from_slice(&page); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real 3.15.3 database, written by python-legacy's `RecipeStore`
    /// (one recipe, one plan with two steps, a constraint, two trajectories).
    fn fixture() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trammel-3.15.3.db")
    }

    #[test]
    fn reads_a_python_sqlite_database() {
        let doc = import_database(&fixture()).unwrap();
        for (table, rows) in [("recipes", 1), ("plans", 1), ("steps", 2), ("constraints", 1), ("trajectories", 2)] {
            assert_eq!(row_count(&doc, table), rows, "{table}");
        }
        let plan = &crate::canon::get(&doc, "plans").and_then(crate::canon::as_array).unwrap()[0];
        assert_eq!(crate::canon::get(plan, "goal").and_then(crate::canon::as_str), Some("add a low-stock report"));
    }

    #[test]
    fn a_migrated_store_keeps_ids_and_history() {
        let dir = std::env::temp_dir().join(format!("trammel-migrated-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let store_dir = dir.join(".trammel");
        {
            let mut store = crate::store::Store::open(&store_dir).unwrap();
            store.replace_document(import_database(&fixture()).unwrap());
            // New rows must not reuse the migrated ids.
            let next = store.create_plan("another goal", &crate::canon::obj(vec![]));
            assert_eq!(next, 2);
        }
        let args = crate::canon::obj(vec![("project_root", Value::string(dir.display().to_string())), ("plan_id", Value::Int(1))]);
        let history = crate::tools::call("history", &args, &dir, Some(&store_dir), false).unwrap();
        assert_eq!(crate::canon::get(&history, "trajectories").and_then(crate::canon::as_array).map(|t| t.len()), Some(2));
        let plan = crate::tools::call("get_plan", &args, &dir, Some(&store_dir), false).unwrap();
        assert_eq!(crate::canon::get(&plan, "steps").and_then(crate::canon::as_array).map(|s| s.len()), Some(2));
        let _ = fs::remove_dir_all(&dir);
    }
}
