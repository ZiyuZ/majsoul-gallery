use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug)]
enum Value {
    Text(String),
    Object(BTreeMap<String, Value>),
}
impl Value {
    fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(map) => map.get(key),
            _ => None,
        }
    }
    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }
}

fn parse(source: &str) -> Result<Value> {
    let mut chars = source.trim_start_matches('\u{feff}').chars().peekable();
    let mut tokens = Vec::new();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        if matches!(c, '{' | '}') {
            tokens.push(c.to_string());
            continue;
        }
        if c == '"' {
            let mut value = String::new();
            let mut closed = false;
            while let Some(c) = chars.next() {
                if c == '"' {
                    closed = true;
                    break;
                }
                if c == '\\' && matches!(chars.peek(), Some('"' | '\\')) {
                    value.push(chars.next().unwrap());
                } else {
                    value.push(c);
                }
            }
            ensure!(closed, "Steam 配置字符串未闭合");
            tokens.push(value);
        } else {
            let mut value = c.to_string();
            while chars
                .peek()
                .is_some_and(|c| !c.is_whitespace() && !matches!(c, '{' | '}'))
            {
                value.push(chars.next().unwrap());
            }
            tokens.push(value);
        }
    }
    fn object(tokens: &[String], cursor: &mut usize, nested: bool) -> Result<Value> {
        let mut map = BTreeMap::new();
        while let Some(key) = tokens.get(*cursor) {
            if key == "}" {
                ensure!(nested, "Steam 配置括号不匹配");
                *cursor += 1;
                return Ok(Value::Object(map));
            }
            ensure!(key != "{", "Steam 配置缺少键名");
            let key = key.clone();
            *cursor += 1;
            let value = tokens.get(*cursor).context("Steam 配置缺少值")?;
            *cursor += 1;
            let value = if value == "{" {
                object(tokens, cursor, true)?
            } else {
                ensure!(value != "}", "Steam 配置缺少值");
                Value::Text(value.clone())
            };
            map.insert(key, value);
        }
        ensure!(!nested, "Steam 配置括号未闭合");
        Ok(Value::Object(map))
    }
    object(&tokens, &mut 0, false)
}

#[cfg(windows)]
fn registered_steam() -> Vec<PathBuf> {
    use windows_sys::Win32::System::Registry::*;
    let mut paths = Vec::new();
    for (root, key, name) in [
        (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Valve\Steam",
            "InstallPath",
        ),
    ] {
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let (key, name) = (wide(key), wide(name));
        let mut buffer = [0u16; 2048];
        let mut size = std::mem::size_of_val(&buffer) as u32;
        let result = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if result == 0 {
            let count = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            paths.push(PathBuf::from(String::from_utf16_lossy(&buffer[..count])));
        }
    }
    paths
}

pub fn find_installs(saved: Option<&Path>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut add = |path: &Path| {
        if let Ok(source) = majsoul_assets::core::index::source_dir(&path.to_string_lossy())
            && source
                .parent()
                .and_then(Path::parent)
                .is_some_and(|data| data.join("resources.assets").is_file())
            && !found.contains(&source)
        {
            found.push(source);
        }
    };
    if let Some(saved) = saved {
        add(saved);
    }
    #[cfg(windows)]
    let mut roots = registered_steam();
    #[cfg(not(windows))]
    let mut roots = Vec::new();
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(path) = std::env::var_os(variable) {
            roots.push(PathBuf::from(path).join("Steam"));
        }
    }
    let mut libraries = roots.clone();
    for root in roots {
        if let Ok(source) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf"))
            && let Ok(value) = parse(&source)
            && let Some(Value::Object(map)) = value.get("libraryfolders")
        {
            for library in map.values() {
                if let Some(path) = library
                    .get("path")
                    .and_then(Value::text)
                    .or_else(|| library.text())
                {
                    libraries.push(PathBuf::from(path));
                }
            }
        }
    }
    libraries.sort();
    libraries.dedup();
    for library in libraries {
        add(&library.join("steamapps/common/MahjongSoul"));
        if let Ok(entries) = fs::read_dir(library.join("steamapps")) {
            for file in entries.filter_map(Result::ok).filter(|e| {
                let name = e.file_name().to_string_lossy().to_lowercase();
                name.starts_with("appmanifest_") && name.ends_with(".acf")
            }) {
                if let Ok(source) = fs::read_to_string(file.path())
                    && let Ok(value) = parse(&source)
                    && let Some(directory) = value
                        .get("AppState")
                        .and_then(|v| v.get("installdir"))
                        .and_then(Value::text)
                    && Path::new(directory)
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                {
                    add(&library.join("steamapps/common").join(directory));
                }
            }
        }
    }
    found
}

pub fn validate(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        bail!("所选路径不存在");
    }
    majsoul_assets::core::index::source_dir(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_steam_libraries_and_unicode_install_names() -> Result<()> {
        let value = parse(
            r#""libraryfolders" { "0" { "path" "D:\\Steam" } "1" { "path" "E:\\游戏" } } // comment"#,
        )?;
        assert_eq!(
            value
                .get("libraryfolders")
                .and_then(|v| v.get("1"))
                .and_then(|v| v.get("path"))
                .and_then(Value::text),
            Some("E:\\游戏")
        );
        let value = parse(r#""AppState" { "installdir" "MahjongSoul" }"#)?;
        assert_eq!(
            value
                .get("AppState")
                .and_then(|v| v.get("installdir"))
                .and_then(Value::text),
            Some("MahjongSoul")
        );
        assert!(parse(r#""libraryfolders" { "0" "D:" "#).is_err());
        Ok(())
    }
}
