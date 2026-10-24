//! Shared report selection and grouping; HTML and PNG only handle presentation.
use super::*;

pub(super) struct GiftGroup<'a> {
    pub gift: &'a Item,
    pub characters: Vec<&'a Character>,
}

pub(super) fn gift_groups<'a>(
    library: &'a Library,
    options: &GiftOptions,
) -> Result<Vec<GiftGroup<'a>>> {
    let mut groups = BTreeMap::<&str, GiftGroup<'a>>::new();
    for character in library.characters.iter().filter(|c| options.includes(c)) {
        if let Some(gift) = purple_gift(character) {
            groups
                .entry(&gift.id)
                .or_insert_with(|| GiftGroup {
                    gift,
                    characters: vec![],
                })
                .characters
                .push(character);
        }
    }
    ensure!(!groups.is_empty(), "所选范围没有可导出的角色喜好资料");
    Ok(groups.into_values().collect())
}

pub(super) struct OwnedEntry<'a> {
    pub name: &'a str,
    pub subtitle: &'a str,
    pub image: Option<PathBuf>,
    pub terms: Vec<&'a str>,
    pub limited: bool,
}

pub(super) struct OwnedSection<'a> {
    pub kind: &'static str,
    pub label: &'static str,
    pub entries: Vec<OwnedEntry<'a>>,
}

pub(super) fn owned_sections<'a>(
    library: &'a Library,
    account: &AccountSnapshot,
    options: &OwnedOptions,
) -> Result<Vec<OwnedSection<'a>>> {
    ensure!(
        options.characters || options.skins || options.decorations,
        "请选择导出的收藏类型"
    );
    let mut sections = Vec::new();
    if options.characters {
        sections.push(OwnedSection {
            kind: "characters",
            label: "角色",
            entries: library
                .characters
                .iter()
                .filter(|c| account.character(&c.id).owned)
                .map(|c| OwnedEntry {
                    name: &c.name,
                    subtitle: if c.sex == 1 { "樱花" } else { "竹林" },
                    image: c.thumbnail(),
                    terms: c.search.terms().to_vec(),
                    limited: c.limited,
                })
                .collect(),
        });
    }
    if options.skins {
        sections.push(OwnedSection {
            kind: "skins",
            label: "皮肤",
            entries: library
                .characters
                .iter()
                .flat_map(|c| {
                    c.skins
                        .iter()
                        .filter(|s| account.owns_skin(s))
                        .map(move |skin| {
                            let mut terms = skin.search.terms().to_vec();
                            terms.extend(c.search.terms());
                            OwnedEntry {
                                name: skin.display_name(&c.name),
                                subtitle: &c.name,
                                image: skin.thumbnail(),
                                terms,
                                limited: c.limited,
                            }
                        })
                })
                .collect(),
        });
    }
    if options.decorations {
        let mut groups = BTreeMap::<&str, Vec<OwnedEntry<'a>>>::new();
        for item in library
            .decorations
            .iter()
            .filter(|item| account.owns_decoration(item))
        {
            let label = crate::text::decoration_name(&item.category);
            groups.entry(label).or_default().push(OwnedEntry {
                name: &item.name,
                subtitle: label,
                image: item.image.clone(),
                terms: item.search.terms().to_vec(),
                limited: false,
            });
        }
        sections.extend(groups.into_iter().map(|(label, entries)| OwnedSection {
            kind: "decorations",
            label,
            entries,
        }));
    }
    Ok(sections)
}
