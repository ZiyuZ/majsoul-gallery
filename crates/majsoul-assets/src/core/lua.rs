//! Bounded reader for generated Excel data literals. Never executes Lua.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            bytes: source.as_bytes(),
            pos: 0,
        }
    }
    fn whitespace(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.pos += 1;
        }
    }
    fn take(&mut self, byte: u8) -> Result<()> {
        self.whitespace();
        ensure!(
            self.bytes.get(self.pos) == Some(&byte),
            "Lua literal: expected {} at {}",
            byte as char,
            self.pos
        );
        self.pos += 1;
        Ok(())
    }
    fn string(&mut self) -> Result<String> {
        let quote = self.bytes[self.pos];
        self.pos += 1;
        let mut out = vec![];
        while let Some(&ch) = self.bytes.get(self.pos) {
            self.pos += 1;
            if ch == quote {
                return Ok(String::from_utf8(out)?);
            }
            if ch != b'\\' {
                out.push(ch);
                continue;
            }
            let ch = *self.bytes.get(self.pos).context("Truncated Lua escape")?;
            self.pos += 1;
            match ch {
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'a' => out.push(7),
                b'b' => out.push(8),
                b'f' => out.push(12),
                b'v' => out.push(11),
                b'0'..=b'9' => {
                    let mut n = (ch - b'0') as u16;
                    for _ in 0..2 {
                        match self.bytes.get(self.pos) {
                            Some(c) if c.is_ascii_digit() => {
                                n = n * 10 + (c - b'0') as u16;
                                self.pos += 1;
                            }
                            _ => break,
                        }
                    }
                    ensure!(n <= 255, "Lua byte escape exceeds 255");
                    out.push(n as u8);
                }
                b'\\' | b'\'' | b'"' | b'\n' => out.push(ch),
                _ => bail!("Unsupported Lua escape {}", ch as char),
            }
        }
        bail!("Unterminated Lua string")
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        ensure!(depth < 64, "Lua literal nesting limit exceeded");
        self.whitespace();
        match self
            .bytes
            .get(self.pos)
            .copied()
            .context("Missing Lua value")?
        {
            b'"' | b'\'' => Ok(Value::String(self.string()?)),
            b'{' => {
                self.pos += 1;
                let mut array = vec![];
                let mut object = Map::new();
                loop {
                    self.whitespace();
                    if self.bytes.get(self.pos) == Some(&b'}') {
                        self.pos += 1;
                        break;
                    }
                    if self.bytes.get(self.pos) == Some(&b'[') {
                        self.pos += 1;
                        let key = self
                            .value(depth + 1)?
                            .as_str()
                            .context("Expected string field")?
                            .to_owned();
                        self.take(b']')?;
                        self.take(b'=')?;
                        object.insert(key, self.value(depth + 1)?);
                    } else {
                        array.push(self.value(depth + 1)?);
                    }
                    self.whitespace();
                    if matches!(self.bytes.get(self.pos), Some(b',' | b';')) {
                        self.pos += 1;
                    }
                }
                ensure!(
                    array.is_empty() || object.is_empty(),
                    "Mixed Lua tables are unsupported"
                );
                Ok(if object.is_empty() {
                    Value::Array(array)
                } else {
                    Value::Object(object)
                })
            }
            b'-' | b'0'..=b'9' => {
                let start = self.pos;
                while self
                    .bytes
                    .get(self.pos)
                    .is_some_and(|c| c.is_ascii_digit() || b"-.+eE".contains(c))
                {
                    self.pos += 1;
                }
                Ok(serde_json::from_slice(&self.bytes[start..self.pos])?)
            }
            _ => {
                let start = self.pos;
                while self
                    .bytes
                    .get(self.pos)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
                {
                    self.pos += 1;
                }
                ensure!(self.pos > start, "Unexpected Lua token at {}", self.pos);
                let name = &self.bytes[start..self.pos];
                match name {
                    b"nil" => return Ok(Value::Null),
                    b"false" => return Ok(json!(false)),
                    b"true" => return Ok(json!(true)),
                    _ => {}
                }
                self.whitespace();
                if self.bytes.get(self.pos) == Some(&b'[') {
                    self.pos += 1;
                    let range = self
                        .value(depth + 1)?
                        .as_u64()
                        .context("Invalid Dmap range")?;
                    self.take(b']')?;
                    Ok(json!({"$skip":range}))
                } else if self.bytes.get(self.pos) == Some(&b'(') {
                    self.pos += 1;
                    // Generated Sub({…}) merely wraps a literal array.
                    let value = self.value(depth + 1)?;
                    self.take(b')')?;
                    Ok(value)
                } else {
                    bail!("Unsupported Lua identifier")
                }
            }
        }
    }
}

fn local_literals(source: &str) -> Result<Vec<Value>> {
    let pattern = Regex::new(r"local\s+\w+\s*=\s*\{")?;
    pattern
        .find_iter(source)
        .map(|m| Reader::new(&source[m.end() - 1..]).value(0))
        .collect()
}

pub fn language(source: &str) -> Result<Vec<Value>> {
    local_literals(source)?
        .into_iter()
        .find_map(|v| v.as_array().cloned())
        .context("Missing language array")
}

/// Restores field positions, omitted Dmap ranges, defaults and language keys.
/// Semantics checked against this installation's ExcelTool.lua.
pub fn sheet(
    schema_source: &str,
    sources: &[&str],
    languages: &BTreeMap<String, Vec<Value>>,
    table: &str,
) -> Result<Vec<Value>> {
    let literals = local_literals(schema_source)?;
    let (i, fields) = literals
        .iter()
        .enumerate()
        .find_map(|(i, v)| {
            v.as_object()
                .filter(|m| m.contains_key("id") && m.values().all(Value::is_i64))
                .map(|m| (i, m))
        })
        .context("Missing Excel schema")?;
    let defaults = literals
        .get(i + 1)
        .and_then(Value::as_array)
        .context("Missing Excel defaults")?;
    ensure!(
        fields
            .values()
            .filter_map(Value::as_i64)
            .all(|p| p != 0 && ((-999..0).contains(&p) || (1..2000).contains(&p))),
        "Excel field encoding changed"
    );
    let max = fields
        .values()
        .filter_map(Value::as_i64)
        .map(|i| i.unsigned_abs() as usize % 1000)
        .max()
        .unwrap_or(0);
    ensure!(max < 1000, "Invalid Excel field count");
    let assignment = Regex::new(r"\b\w+\[\d+\]\s*=\s*\w+\(\s*\{")?;
    let mut rows = vec![];
    for source in sources {
        for found in assignment.find_iter(source) {
            let data = Reader::new(&source[found.end() - 1..]).value(0)?;
            let mut positions = defaults.clone();
            positions.resize(max, Value::Null);
            let mut position = 1usize;
            let mut skip = None;
            for value in data.as_array().context("Missing packed row")? {
                if let Some(range) = value.get("$skip").and_then(Value::as_u64) {
                    ensure!(skip.is_none(), "Multiple Dmap ranges in one row");
                    skip = Some(((range / 1000) as usize, (range % 1000) as usize));
                    continue;
                }
                if let Some((start, end)) = skip
                    && position >= start
                    && position <= end
                {
                    position = end + 1;
                }
                ensure!(
                    position <= max,
                    "Packed row exceeds field layout in {table}"
                );
                if value != &Value::Bool(false) {
                    positions[position - 1] = value.clone();
                }
                position += 1;
            }
            let mut row = Map::new();
            for (field, raw) in fields {
                let raw = raw.as_i64().context("Invalid field position")?;
                let pos = raw.unsigned_abs() as usize % 1000;
                let mut value = positions.get(pos - 1).cloned().unwrap_or_default();
                if raw < 0 {
                    value = value
                        .as_u64()
                        .and_then(|key| key.checked_sub(1))
                        .and_then(|key| {
                            languages
                                .get(&format!("{table}_{field}"))
                                .and_then(|v| v.get(key as usize))
                        })
                        .cloned()
                        .unwrap_or(json!(""));
                } else if value.is_null() || value == Value::Bool(false) {
                    value = if raw >= 1000 { json!("") } else { json!(0) };
                }
                row.insert(field.clone(), value);
            }
            rows.push(Value::Object(row));
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_omitted_fields_and_localized_values() {
        let schema = r#"local f={["id"]=1,["name_chs"]=-2,["path"]=1003,["type"]=4}local d={nil,nil,"default",7}"#;
        let rows = sheet(
            schema,
            &[r#"g[9]=b({e[3004],9,1},h)g[10]=b({10,2,false,0},h)"#],
            &BTreeMap::from([(
                "item_definition_skin_name_chs".into(),
                vec![json!("一姬"), json!("契约")],
            )]),
            "item_definition_skin",
        )
        .unwrap();
        assert_eq!(rows[0]["name_chs"], "一姬");
        assert_eq!(rows[0]["path"], "default");
        assert_eq!(rows[0]["type"], 7);
        assert_eq!(rows[1]["name_chs"], "契约");
        assert_eq!(rows[1]["type"], 0);
        assert!(Reader::new("os.execute('anything')").value(0).is_err());
    }
}
