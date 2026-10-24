mod image_report;
use crate::{
    account::AccountSnapshot,
    model::{Character, Item, Library},
};
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    collections::BTreeMap,
    io::Cursor,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct GiftOptions {
    pub sakura: bool,
    pub bamboo: bool,
    pub permanent: bool,
    pub limited: bool,
}
impl Default for GiftOptions {
    fn default() -> Self {
        Self {
            sakura: true,
            bamboo: true,
            permanent: true,
            limited: true,
        }
    }
}
impl GiftOptions {
    fn includes(&self, c: &Character) -> bool {
        ((c.sex == 1 && self.sakura) || (c.sex == 2 && self.bamboo))
            && ((!c.limited && self.permanent) || (c.limited && self.limited))
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn purple_gift(c: &Character) -> Option<&Item> {
    c.gifts.iter().find(|gift| {
        c.materials
            .iter()
            .any(|g| g.items.iter().any(|i| i.id == gift.id))
    })
}
fn embedded_image(path: Option<PathBuf>, cache: &mut BTreeMap<PathBuf, String>) -> Result<String> {
    let Some(path) = path else {
        return Ok(String::new());
    };
    if let Some(value) = cache.get(&path) {
        return Ok(value.clone());
    }
    let image = image::open(&path)?.thumbnail(144, 144);
    let mut buffer = Cursor::new(Vec::new());
    image.write_to(&mut buffer, image::ImageFormat::Png)?;
    let value = format!(
        "<img alt=\"\" src=\"data:image/png;base64,{}\">",
        STANDARD.encode(buffer.into_inner())
    );
    cache.insert(path, value.clone());
    Ok(value)
}
pub fn gift_html(library: &Library, options: &GiftOptions) -> Result<String> {
    let mut groups = BTreeMap::<String, (&Item, Vec<&Character>)>::new();
    let mut selected = 0;
    for c in library.characters.iter().filter(|c| options.includes(c)) {
        if let Some(gift) = purple_gift(c) {
            groups
                .entry(gift.id.clone())
                .or_insert_with(|| (gift, vec![]))
                .1
                .push(c);
            selected += 1;
        }
    }
    ensure!(selected > 0, "所选范围没有可导出的角色喜好资料");
    let mut out = String::from(include_str!("assets/gift-report.html"));
    let mut cache = BTreeMap::new();
    out.push_str(&format!("<p class=\"summary\"><span id=\"result-count\" role=\"status\" aria-live=\"polite\">共 {selected} 位角色</span> · 金色边框表示限定角色</p><p id=\"report-empty\" class=\"empty\" hidden>没有符合条件的角色，请调整搜索或筛选。</p><div class=\"head\"><h2>樱花</h2><h2>紫色礼物</h2><h2>竹林</h2></div>"));
    for (_, (gift, characters)) in groups {
        out.push_str("<section class=\"row\">");
        for sex in [1, 2] {
            if sex == 2 {
                out.push_str(&format!(
                    "<div class=\"gift\">{}<strong>{}</strong></div>",
                    embedded_image(gift.icon.clone(), &mut cache)?,
                    escape(&gift.name)
                ));
            }
            out.push_str("<div class=\"characters\">");
            for c in characters.iter().filter(|c| c.sex == sex) {
                let art = c.thumbnail();
                out.push_str(&format!(
                    "<article class=\"character{}\" title=\"{}\" data-limited=\"{}\" data-search=\"{}\">{}<span>{}</span></article>",
                    if c.limited { " limited" } else { "" },
                    escape(&c.name),
                    c.limited,
                    escape(&serde_json::to_string(&c.search.terms())?),
                    embedded_image(art, &mut cache)?,
                    escape(&c.name)
                ));
            }
            out.push_str("</div>");
        }
        out.push_str("</section>");
    }
    out.push_str("</main><script>");
    out.push_str(include_str!("assets/report-search.js"));
    out.push_str(include_str!("assets/gift-report.js"));
    out.push_str("</script></body></html>");
    Ok(out)
}
pub fn write_gifts(library: &Library, options: &GiftOptions, path: &Path) -> Result<()> {
    if is_png(path) {
        image_report::gifts(library, options, path)
    } else {
        write_html(&gift_html(library, options)?, path)
    }
}
fn write_html(html: &str, path: &Path) -> Result<()> {
    crate::files::write_atomic(path, html.as_bytes())
}

#[derive(Clone)]
pub struct OwnedOptions {
    pub characters: bool,
    pub skins: bool,
    pub decorations: bool,
}
impl Default for OwnedOptions {
    fn default() -> Self {
        Self {
            characters: true,
            skins: true,
            decorations: true,
        }
    }
}
pub fn owned_html(
    library: &Library,
    account: &AccountSnapshot,
    options: &OwnedOptions,
) -> Result<String> {
    ensure!(
        options.characters || options.skins || options.decorations,
        "请选择导出的收藏类型"
    );
    let mut cache = BTreeMap::new();
    let mut sections = Vec::<(&str, &str, Vec<String>)>::new();
    let mut card = |name: &str,
                    subtitle: &str,
                    image: Option<PathBuf>,
                    terms: Vec<&str>,
                    limited: bool|
     -> Result<String> {
        Ok(format!(
            "<article class=\"entry{}\" data-search=\"{}\">{}<strong>{}</strong><span>{}</span></article>",
            if limited { " limited" } else { "" },
            escape(&serde_json::to_string(&terms)?),
            embedded_image(image, &mut cache)?,
            escape(name),
            escape(subtitle)
        ))
    };
    if options.characters {
        let mut cards = Vec::new();
        for c in library
            .characters
            .iter()
            .filter(|c| account.characters.get(&c.id).is_some_and(|s| s.owned))
        {
            cards.push(card(
                &c.name,
                if c.sex == 1 { "樱花" } else { "竹林" },
                c.thumbnail(),
                c.search.terms().to_vec(),
                c.limited,
            )?);
        }
        sections.push(("characters", "角色", cards));
    }
    if options.skins {
        let mut cards = Vec::new();
        for c in &library.characters {
            for skin in c.skins.iter().filter(|skin| account.owns_skin(skin)) {
                let mut terms = skin.search.terms().to_vec();
                terms.extend(c.search.terms());
                cards.push(card(
                    skin.display_name(&c.name),
                    &c.name,
                    skin.thumbnail(),
                    terms,
                    c.limited,
                )?);
            }
        }
        sections.push(("skins", "皮肤", cards));
    }
    if options.decorations {
        let mut groups = BTreeMap::<&str, Vec<String>>::new();
        for d in library
            .decorations
            .iter()
            .filter(|d| account.owns_decoration(d))
        {
            groups
                .entry(crate::text::decoration_name(&d.category))
                .or_default()
                .push(card(
                    &d.name,
                    crate::text::decoration_name(&d.category),
                    d.image.clone(),
                    d.search.terms().to_vec(),
                    false,
                )?);
        }
        for (label, cards) in groups {
            sections.push(("decorations", label, cards));
        }
    }
    let total = sections
        .iter()
        .map(|(_, _, cards)| cards.len())
        .sum::<usize>();
    let mut out = String::from(include_str!("assets/owned-report.html"));
    out.push_str(&format!(
        "<header><h1>{}的收藏</h1><p>UID {}{}</p></header>",
        escape(&account.profile.nickname),
        escape(&account.profile.uid),
        if account.is_demo {
            " · 示例账号预览"
        } else {
            ""
        }
    ));
    out.push_str("<section id=\"owned-controls\" class=\"toolbar\" hidden><input id=\"owned-search\" type=\"search\" placeholder=\"搜索名称、拼音、首字母或 ID…\" aria-label=\"搜索收藏\"><div class=\"filters\">");
    out.push_str("<button type=\"button\" data-kind=\"all\" aria-pressed=\"true\">全部</button>");
    for (kind, name, enabled) in [
        ("characters", "角色", options.characters),
        ("skins", "皮肤", options.skins),
        ("decorations", "装扮", options.decorations),
    ] {
        if enabled {
            out.push_str(&format!("<button type=\"button\" data-kind=\"{kind}\" aria-pressed=\"false\">{name}</button>"));
        }
    }
    out.push_str("</div><button id=\"owned-reset\" type=\"button\">重置</button></section>");
    out.push_str(&format!("<p id=\"owned-count\" role=\"status\" aria-live=\"polite\">共 {total} 项收藏</p><p id=\"owned-empty\" class=\"empty\" hidden>没有符合条件的收藏</p>"));
    let mut decoration_heading = false;
    for (kind, name, cards) in sections {
        if kind == "decorations" && !decoration_heading {
            out.push_str("<h2 id=\"decorations-heading\">装扮</h2>");
            decoration_heading = true;
        }
        let heading = if kind == "decorations" { "h3" } else { "h2" };
        out.push_str(&format!("<section class=\"owned-section\" data-kind=\"{kind}\"><{heading}>{name} <small>{}</small></{heading}><div class=\"gallery\">{}</div></section>",cards.len(),cards.join("")));
    }
    out.push_str("</main><script>");
    out.push_str(include_str!("assets/report-search.js"));
    out.push_str(include_str!("assets/owned-report.js"));
    out.push_str("</script></body></html>");
    Ok(out)
}
pub fn write_owned(
    library: &Library,
    account: &AccountSnapshot,
    options: &OwnedOptions,
    path: &Path,
) -> Result<()> {
    if is_png(path) {
        image_report::owned(library, account, options, path)
    } else {
        write_html(&owned_html(library, account, options)?, path)
    }
}

fn is_png(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_export_respects_snapshot_and_selected_categories() -> Result<()> {
        use crate::{
            model::{Decoration, Skin},
            search::SearchIndex,
        };
        let skin = |id: &str, name: &str| Skin {
            search: SearchIndex::new(name, id),
            id: id.into(),
            name: name.into(),
            full: None,
            portrait: None,
            avatar: None,
            animated: false,
            scene: None,
        };
        let decoration = |id: &str, name: &str| Decoration {
            search: SearchIndex::new(name, id),
            id: id.into(),
            name: name.into(),
            category: "frame".into(),
            image: None,
        };
        let library = Library {
            characters: vec![Character {
                search: SearchIndex::new("一姬", "1"),
                id: "1".into(),
                name: "一姬".into(),
                sort: 0,
                sex: 1,
                limited: false,
                collaboration: false,
                skins: vec![skin("skin1", "一姬·初始"), skin("skin2", "未拥有皮肤")],
                gifts: vec![],
                materials: vec![],
            }],
            decorations: vec![
                decoration("deco1", "已拥有头像框"),
                decoration("deco2", "未拥有头像框"),
            ],
            ..Default::default()
        };
        let mut account = AccountSnapshot::demo(&library);
        account.owned_skins = ["skin1".into()].into_iter().collect();
        account.owned_decorations = ["deco1".into()].into_iter().collect();
        let account: AccountSnapshot = serde_json::from_slice(&serde_json::to_vec(&account)?)?;
        assert!(account.owns_decoration(&library.decorations[0]));
        let html = owned_html(&library, &account, &OwnedOptions::default())?;
        assert!(html.contains("示例账号预览"));
        assert!(html.contains("已拥有头像框"));
        assert!(!html.contains("未拥有头像框"));
        assert!(!html.contains("未拥有皮肤"));
        let html = owned_html(
            &library,
            &account,
            &OwnedOptions {
                characters: false,
                skins: true,
                decorations: false,
            },
        )?;
        assert!(html.contains("data-kind=\"skins\""));
        assert!(!html.contains("data-kind=\"characters\""));
        assert!(!html.contains("已拥有头像框"));
        let output = tempfile::tempdir()?;
        let path = output.path().join("owned.png");
        write_owned(&library, &account, &OwnedOptions::default(), &path)?;
        assert_eq!(image::image_dimensions(path)?.0, 1600);
        Ok(())
    }
    #[test]
    fn exported_library_produces_a_portable_gift_sheet() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".local/target/debug/majsoul-gallery-data/materials");
        if !root.is_dir() {
            return Ok(());
        }
        let library = Library::load(&root)?;
        let temporary = tempfile::tempdir()?;
        let file = temporary.path().join("gifts.html");
        write_gifts(&library, &GiftOptions::default(), &file)?;
        let html = std::fs::read_to_string(file)?;
        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("一姬"));
        assert!(!html.contains("file:///"));
        // Base64 image data may contain the letters "http" by coincidence.
        assert!(!html.contains("src=\"http"));
        assert!(!html.contains("url(http"));
        assert_eq!(html.matches("class=\"gift\"").count(), 8);
        Ok(())
    }
    #[test]
    fn gift_selection_uses_contract_reference_and_escapes_names() -> Result<()> {
        use crate::{model::Requirement, search::SearchIndex};
        let gift = |id: &str, name: &str| Item {
            id: id.into(),
            name: name.into(),
            icon: None,
        };
        let c = Character {
            id: "1".into(),
            name: "<角色>".into(),
            search: SearchIndex::new("<角色>", "1"),
            sort: 1,
            sex: 1,
            limited: true,
            collaboration: false,
            skins: vec![],
            gifts: vec![
                gift("green", "绿"),
                gift("purple", "紫&礼物"),
                gift("locked", "锁"),
            ],
            materials: vec![Requirement {
                quantity: 5,
                items: vec![gift("purple", "紫&礼物")],
            }],
        };
        assert_eq!(purple_gift(&c).unwrap().id, "purple");
        assert!(
            !GiftOptions {
                limited: false,
                ..Default::default()
            }
            .includes(&c)
        );
        let library = Library {
            characters: vec![c],
            ..Default::default()
        };
        let html = gift_html(&library, &GiftOptions::default())?;
        assert!(html.contains("&lt;角色&gt;"));
        assert!(html.contains("紫&amp;礼物"));
        assert!(html.contains("data-limited=\"true\""));
        let terms = SearchIndex::new("<角色>", "1");
        assert!(html.contains(&format!(
            "data-search=\"{}\"",
            escape(&serde_json::to_string(&terms.terms())?)
        )));
        assert!(!html.contains("<span>锁</span>"));
        let output = tempfile::tempdir()?;
        let path = output.path().join("gifts.png");
        write_gifts(&library, &GiftOptions::default(), &path)?;
        assert_eq!(image::image_dimensions(path)?.0, 1600);
        Ok(())
    }
}
