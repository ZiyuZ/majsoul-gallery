use pinyin::ToPinyin;

pub fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| if c == 'ü' { 'v' } else { c })
        .collect()
}

/// Built once with the asset metadata and reused while typing.
#[derive(Clone, Debug)]
pub struct SearchIndex {
    name: String,
    id: String,
    pinyin: String,
    initials: String,
}
impl SearchIndex {
    pub fn new(name: &str, id: &str) -> Self {
        let mut full = String::new();
        let mut initials = String::new();
        for character in normalize(name).chars() {
            if let Some(pinyin) = character.to_pinyin() {
                let syllable = pinyin.plain();
                full.push_str(syllable);
                if let Some(initial) = syllable.chars().next() {
                    initials.push(initial);
                }
            } else {
                full.push(character);
                initials.push(character);
            }
        }
        Self {
            name: normalize(name),
            id: normalize(id),
            pinyin: normalize(&full),
            initials,
        }
    }
    pub fn matches(&self, query: &str) -> bool {
        self.name.contains(query)
            || self.id.contains(query)
            || self.pinyin.contains(query)
            || self.initials.contains(query)
    }
    pub fn terms(&self) -> [&str; 4] {
        [&self.name, &self.id, &self.pinyin, &self.initials]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_names_ids_full_pinyin_and_initials() {
        let index = SearchIndex::new("间桐樱 · 命运之夜", "40010803");
        for query in [
            "间桐樱",
            "jiantongying",
            "JTY",
            "jian tong ying",
            "命运",
            "myzy",
            "40010803",
        ] {
            assert!(index.matches(&normalize(query)), "{query}");
        }
        assert!(!index.matches("yiji"));
    }

    #[test]
    fn preserves_latin_names_and_handles_empty_queries() {
        let index = SearchIndex::new("Fate / 纯真宝玉", "302001");
        for query in ["FATE", "chunzhenbaoyu", "czby", "302001", ""] {
            assert!(index.matches(&normalize(query)), "{query}");
        }
    }
}
