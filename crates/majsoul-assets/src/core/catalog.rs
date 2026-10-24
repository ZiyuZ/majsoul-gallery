use super::index::{Asset, Library, digest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize)]
pub struct Category {
    pub parent: Option<String>,
    pub id: String,
    pub label: String,
    pub icon: String,
    pub groups: usize,
    pub files: usize,
}
#[derive(Clone, Serialize)]
pub struct Resource {
    #[serde(skip)]
    pub item: Option<super::characters::NamedItem>,
    pub id: String,
    pub path: String,
    pub label: String,
    pub role: String,
    pub class: String,
    pub bindings: Vec<super::names::Binding>,
    #[serde(skip)]
    pub bundle: String,
    #[serde(skip)]
    pub asset: Option<Asset>,
}
#[derive(Clone, Serialize)]
pub struct Group {
    #[serde(skip)]
    pub character_sort: Option<i64>,
    pub parent: Option<GroupParent>,
    pub story: bool,
    pub character: Option<super::characters::Character>,
    pub id: String,
    pub label: String,
    pub path: String,
    pub category: String,
    pub bindings: Vec<super::names::Binding>,
    pub resources: Vec<Resource>,
}
#[derive(Clone, Serialize)]
pub struct GroupParent {
    pub id: String,
    pub label: String,
}
#[derive(Serialize)]
pub struct Page {
    pub groups: Vec<Group>,
    pub total: usize,
    pub files: usize,
    pub page: usize,
    pub pages: usize,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct CharacterFilter {
    pub sex: Option<i64>,
    pub limited_only: bool,
    pub collaboration_only: bool,
}
impl CharacterFilter {
    fn matches(&self, group: &Group) -> bool {
        let character = group.character.as_ref();
        self.sex
            .is_none_or(|sex| character.and_then(|c| c.sex) == Some(sex))
            && (!self.limited_only || character.and_then(|c| c.limited) == Some(true))
            && (!self.collaboration_only
                || character
                    .and_then(|c| c.collaboration)
                    .is_some_and(|v| v > 0))
    }
}

pub struct Catalog {
    pub categories: Vec<Category>,
    pub groups: Vec<Group>,
    pub resources: BTreeMap<String, Resource>,
}
const CATEGORIES: &[(&str, &str, &str, &str)] = &[
    (
        "character",
        "角色与服饰",
        "users-round",
        "myassets/deco/character/",
    ),
    (
        "background",
        "大厅背景",
        "panels-top-left",
        "myassets/extendres/background/lobby/",
    ),
    (
        "background-icon",
        "背景图标",
        "image",
        "myassets/deco/lobby_background/",
    ),
    ("hand", "手模", "hand", "myassets/deco/hand/"),
    ("frame", "头像框", "scan-face", "myassets/deco/head_frame/"),
    (
        "stick",
        "立直棒",
        "wand-sparkles",
        "myassets/deco/lizhibang/",
    ),
    ("tile-back", "牌背", "layers-2", "myassets/deco/mjpai/"),
    ("tile-face", "牌面", "grid-2x2", "myassets/deco/mjpface/"),
    ("table", "桌布", "layout-grid", "myassets/deco/tablecloth/"),
    ("title", "称号", "badge", "myassets/deco/title/"),
    (
        "win-effect",
        "和牌特效",
        "sparkles",
        "myassets/deco/effect_hupai/",
    ),
    (
        "riichi-effect",
        "立直特效",
        "zap",
        "myassets/deco/effect_lizhi/",
    ),
    (
        "call-effect",
        "鸣牌特效",
        "message-circle",
        "myassets/deco/effect_mingpaizhishi/",
    ),
    ("bgm", "BGM 图标", "music", "myassets/deco/bgm/"),
    (
        "riichi-bgm",
        "立直 BGM 图标",
        "music-2",
        "myassets/deco/lizhi_bgm/",
    ),
    ("loading", "加载插画", "images", "myassets/deco/loading_cg/"),
    ("emote", "表情", "smile", "myassets/deco/emo/"),
    ("cutin", "特殊和牌原料", "film", "myassets/deco/cutin/"),
    (
        "lobby-click",
        "大厅点击特效",
        "mouse-pointer-click",
        "myassets/deco/effect_click/",
    ),
    (
        "lobby-effect",
        "大厅背景特效",
        "sparkles",
        "myassets/deco/lobby_background_effect/",
    ),
    (
        "story-background",
        "传记背景",
        "image",
        "myassets/extendres/background/story/",
    ),
    ("font", "字体", "type", "myassets/fonts/"),
    ("login", "登录素材", "log-in", "myassets/scenes/entrance/"),
    ("spine", "动态皮肤原料", "clapperboard", "myassets/spine/"),
];
fn role(path: &str) -> (&str, u8) {
    if path.contains("/full/") {
        ("立绘", 0)
    } else if path.contains("/bighead/") {
        ("大头像", 1)
    } else if path.contains("/smallhead/") {
        ("小头像", 2)
    } else if path.contains("/face/") {
        ("表情", 7)
    } else if path.contains("/3d/") {
        ("模型贴图", 6)
    } else if path.contains("/preview/") {
        ("展示图", 4)
    } else if path.contains("/pic/") || path.contains("/icon/") {
        ("图标", 3)
    } else {
        ("图片", 5)
    }
}
impl Catalog {
    pub fn build(library: &Library) -> Self {
        let mut grouped = BTreeMap::<String, Group>::new();
        let mut resources = BTreeMap::new();
        for (bundle_name, bundle) in &library.index.bundles {
            for asset in &bundle.assets {
                let path = &asset.path;
                let (category, prefix) = CATEGORIES
                    .iter()
                    .find(|(_, _, _, p)| path.starts_with(p))
                    .map(|(id, _, _, p)| (*id, *p))
                    .unwrap_or(("other", "myassets/"));
                let remaining = path.strip_prefix(prefix).unwrap_or(path);
                let folder = remaining.split('/').next().unwrap_or("其他");
                let group_path = format!("{prefix}{folder}");
                let group_id = digest(format!("{category}|{group_path}"));
                let id = digest(format!(
                    "{bundle_name}|{}|{}|{}",
                    asset.file, asset.id, path
                ));
                let resource = Resource {
                    item: None,
                    bindings: vec![],
                    id: id.clone(),
                    path: path.clone(),
                    label: path.rsplit('/').next().unwrap_or(path).into(),
                    role: if category == "font" {
                        "字体"
                    } else {
                        role(path).0
                    }
                    .into(),
                    class: if category == "font" {
                        "Font"
                    } else if asset.class == 213 {
                        "Sprite"
                    } else {
                        "Texture2D"
                    }
                    .into(),
                    bundle: bundle_name.clone(),
                    asset: Some(asset.clone()),
                };
                resources.insert(id, resource.clone());
                grouped
                    .entry(group_id.clone())
                    .or_insert_with(|| Group {
                        character_sort: None,
                        parent: None,
                        story: false,
                        character: None,
                        bindings: vec![],
                        id: group_id,
                        label: folder.into(),
                        path: group_path,
                        category: category.into(),
                        resources: vec![],
                    })
                    .resources
                    .push(resource);
            }
        }
        let mut groups = grouped.into_values().collect::<Vec<_>>();
        groups.sort_by(|a, b| a.path.cmp(&b.path));
        for group in &mut groups {
            group.resources.sort_by(|a, b| {
                (role(&a.path).1, &a.path, &a.id).cmp(&(role(&b.path).1, &b.path, &b.id))
            });
        }
        let mut categories = CATEGORIES
            .iter()
            .map(|(id, label, icon, _)| Category {
                parent: None,
                id: (*id).into(),
                label: (*label).into(),
                icon: (*icon).into(),
                groups: 0,
                files: 0,
            })
            .collect::<Vec<_>>();
        categories.push(Category {
            parent: None,
            id: "other".into(),
            label: "其他资源".into(),
            icon: "folder-archive".into(),
            groups: 0,
            files: 0,
        });
        for cat in &mut categories {
            for group in groups.iter().filter(|g| g.category == cat.id) {
                cat.groups += 1;
                cat.files += group.resources.len();
            }
        }
        Self {
            categories,
            groups,
            resources,
        }
    }
    pub fn page(&self, category: &str, query: &str, page: usize, show_all: bool) -> Page {
        self.page_filtered(category, query, page, show_all, &CharacterFilter::default())
    }
    pub fn page_filtered(
        &self,
        category: &str,
        query: &str,
        page: usize,
        show_all: bool,
        filter: &CharacterFilter,
    ) -> Page {
        let query = query.trim().to_lowercase();
        let matches = self
            .groups
            .iter()
            .filter(|g| {
                matches_category(category, &g.category)
                    && (!matches!(category, "character" | "character-static") || filter.matches(g))
                    && (show_all || g.category != "other")
                    && (query.is_empty()
                        || g.parent
                            .as_ref()
                            .is_some_and(|p| p.label.to_lowercase().contains(&query))
                        || g.label.to_lowercase().contains(&query)
                        || g.bindings.iter().any(|b| {
                            b.name.to_lowercase().contains(&query)
                                || b.id.to_string().contains(&query)
                        })
                        || g.resources
                            .iter()
                            .any(|r| r.path.to_lowercase().contains(&query)))
            })
            .collect::<Vec<_>>();
        if matches!(category, "character" | "character-static") {
            // A character is the pagination unit: its skins must stay together.
            let mut owners = BTreeMap::<(i64, i64, String), Vec<&Group>>::new();
            for group in matches {
                let id = group
                    .parent
                    .as_ref()
                    .map(|p| p.id.as_str())
                    .unwrap_or(&group.id);
                let number = id
                    .strip_prefix("character:")
                    .and_then(|n| n.parse::<i64>().ok())
                    .unwrap_or(i64::MAX);
                owners
                    .entry((
                        group.character_sort.unwrap_or(i64::MAX),
                        number,
                        id.to_owned(),
                    ))
                    .or_default()
                    .push(group);
            }
            let total = owners.len();
            let files = owners.values().flatten().map(|g| g.resources.len()).sum();
            let pages = total.div_ceil(40).max(1);
            let page = page.min(pages - 1);
            let mut groups = Vec::new();
            for mut skins in owners.into_values().skip(page * 40).take(40) {
                skins.sort_by_key(|g| {
                    g.bindings
                        .iter()
                        .find(|b| b.table == "item_definition.skin" || b.table == "spot.skin_spot")
                        .map(|b| b.id)
                        .unwrap_or(i64::MAX)
                });
                groups.extend(skins.into_iter().cloned());
            }
            return Page {
                groups,
                total,
                files,
                page,
                pages,
            };
        }
        let total = matches.len();
        let files = matches.iter().map(|g| g.resources.len()).sum();
        let pages = total.div_ceil(40).max(1);
        let page = page.min(pages - 1);
        let groups = matches
            .into_iter()
            .skip(page * 40)
            .take(40)
            .cloned()
            .collect();
        Page {
            groups,
            total,
            files,
            page,
            pages,
        }
    }
}

pub fn matches_category(selected: &str, actual: &str) -> bool {
    match selected {
        "all" => true,
        "character-static" => actual == "character",
        "extras" => EXTRAS.iter().any(|(id, _, _)| *id == actual),
        "decoration" => DECORATIONS
            .iter()
            .any(|(id, _, _)| matches_category(id, actual)),
        "background" => matches!(actual, "background" | "background-icon"),
        "music" => matches!(actual, "music" | "bgm" | "riichi-bgm"),
        "illustration" => actual == "loading",
        _ => selected == actual,
    }
}
pub const DECORATIONS: &[(&str, &str, &str)] = &[
    ("win-effect", "和牌特效", "sparkles"),
    ("riichi-effect", "立直特效", "zap"),
    ("stick", "立直棒", "wand-sparkles"),
    ("table", "桌布", "layout-grid"),
    ("tile-back", "牌背", "layers-2"),
    ("tile-face", "牌面", "grid-2x2"),
    ("frame", "头像框", "scan-face"),
    ("call-effect", "鸣牌指示", "message-circle"),
    ("hand", "手的样式", "hand"),
    ("background", "大厅背景", "panels-top-left"),
    ("music", "音乐图标", "music"),
];
pub const EXTRAS: &[(&str, &str, &str)] = &[
    ("emote", "表情", "smile"),
    ("cutin", "特殊和牌原料", "film"),
    ("lobby-click", "大厅点击特效", "mouse-pointer-click"),
    ("lobby-effect", "大厅背景特效", "sparkles"),
    ("story-background", "传记背景", "image"),
    ("font", "字体", "type"),
    ("login", "登录素材", "log-in"),
];
impl Catalog {
    pub fn organize(&mut self) {
        let roots = [
            ("character", "角色", "users-round"),
            ("item", "道具", "package"),
            ("gift", "礼物", "gift"),
            ("decoration", "装扮", "palette"),
            ("illustration", "插画", "images"),
            ("title", "称号", "badge"),
            ("rank", "段位", "award"),
            ("extras", "其他素材", "boxes"),
            ("other", "其他资源", "folder-archive"),
        ];
        self.categories = roots
            .iter()
            .map(|&(id, label, icon)| (id, label, icon, None))
            .chain(
                DECORATIONS
                    .iter()
                    .map(|&(id, label, icon)| (id, label, icon, Some("decoration"))),
            )
            .chain(
                EXTRAS
                    .iter()
                    .map(|&(id, label, icon)| (id, label, icon, Some("extras"))),
            )
            .chain([
                ("character-static", "静态皮肤", "image", Some("character")),
                ("spine", "动态皮肤", "clapperboard", Some("character")),
            ])
            .map(|(id, label, icon, parent)| {
                let groups = self
                    .groups
                    .iter()
                    .filter(|g| matches_category(id, &g.category))
                    .collect::<Vec<_>>();
                Category {
                    id: id.into(),
                    label: label.into(),
                    icon: icon.into(),
                    parent: parent.map(str::to_owned),
                    groups: if matches!(id, "character" | "character-static") {
                        groups
                            .iter()
                            .map(|g| g.parent.as_ref().map(|p| p.id.as_str()).unwrap_or(&g.id))
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                    } else {
                        groups.len()
                    },
                    files: groups.iter().map(|g| g.resources.len()).sum(),
                }
            })
            .collect();
        for group in &mut self.groups {
            if let Some((family, &(id, label, _))) = DECORATIONS
                .iter()
                .map(|entry| ("decoration", entry))
                .chain(EXTRAS.iter().map(|entry| ("extras", entry)))
                .find(|(_, (id, _, _))| matches_category(id, &group.category))
            {
                group.parent = Some(GroupParent {
                    id: format!("{family}:{id}"),
                    label: label.into(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roles_distinguish_source_images() {
        assert_eq!(role("a/full/full.png").0, "立绘");
        assert_eq!(role("a/3d/texture/mjp.png").0, "模型贴图");
    }
    #[test]
    fn empty_page_is_valid() {
        let c = Catalog {
            categories: vec![],
            groups: vec![],
            resources: BTreeMap::new(),
        };
        let p = c.page("all", "", 999, false);
        assert_eq!((p.page, p.pages, p.total), (0, 1, 0));
    }
}
