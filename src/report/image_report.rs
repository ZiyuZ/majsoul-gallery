//! Compact, self-contained PNG reports rendered through SVG with system font fallback.
use super::*;
use anyhow::Context;

struct Canvas {
    body: String,
    images: BTreeMap<PathBuf, String>,
}
impl Canvas {
    fn new(title: &str, subtitle: &str) -> Self {
        let mut canvas = Self {
            body: String::new(),
            images: BTreeMap::new(),
        };
        canvas.text(24, 40, title, 28, "#213547");
        canvas.text(24, 70, subtitle, 16, "#617384");
        canvas
    }
    fn text(&mut self, x: u32, y: u32, text: &str, size: u32, color: &str) {
        self.body.push_str(&format!(
            "<text x='{x}' y='{y}' font-size='{size}' fill='{color}'>{}</text>",
            escape(text)
        ));
    }
    fn panel(&mut self, x: u32, y: u32, w: u32, h: u32, limited: bool) {
        self.body.push_str(&format!("<rect x='{x}' y='{y}' width='{w}' height='{h}' rx='6' fill='white' stroke='{}' stroke-width='{}'/>", if limited { "#c99336" } else { "#dce5ec" }, if limited { 2 } else { 1 }));
    }
    fn image(&mut self, path: Option<PathBuf>, x: u32, y: u32, size: u32) -> Result<()> {
        let Some(path) = path else {
            return Ok(());
        };
        if !self.images.contains_key(&path) {
            let image = image::open(&path)?.thumbnail(160, 160);
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, image::ImageFormat::Png)?;
            self.images
                .insert(path.clone(), STANDARD.encode(bytes.into_inner()));
        }
        self.body.push_str(&format!("<image x='{x}' y='{y}' width='{size}' height='{size}' preserveAspectRatio='xMidYMid meet' xlink:href='data:image/png;base64,{}'/>", self.images[&path]));
        Ok(())
    }
    fn save(self, height: u32, path: &Path) -> Result<()> {
        ensure!(
            u64::from(height) * 1600 <= 64_000_000,
            "图片过长，请缩小导出范围后重试"
        );
        let svg = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='1600' height='{height}'><rect width='1600' height='{height}' fill='#f4f6f8'/><g font-family='Microsoft YaHei UI, Noto Sans CJK SC, sans-serif'>{}</g></svg>",
            self.body
        );
        let mut options = resvg::usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        let tree = resvg::usvg::Tree::from_str(&svg, &options)?;
        let mut pixels = resvg::tiny_skia::Pixmap::new(1600, height).context("无法分配图表画布")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::default(),
            &mut pixels.as_mut(),
        );
        crate::files::write_atomic(path, &pixels.encode_png()?)
    }
}
fn short(name: &str, limit: usize) -> String {
    if name.chars().count() <= limit {
        name.into()
    } else {
        format!("{}…", name.chars().take(limit - 1).collect::<String>())
    }
}

pub(super) fn gifts(library: &Library, options: &GiftOptions, path: &Path) -> Result<()> {
    let mut groups = BTreeMap::<String, (&Item, Vec<&Character>)>::new();
    for c in library.characters.iter().filter(|c| options.includes(c)) {
        if let Some(gift) = purple_gift(c) {
            groups
                .entry(gift.id.clone())
                .or_insert_with(|| (gift, vec![]))
                .1
                .push(c);
        }
    }
    ensure!(!groups.is_empty(), "所选范围没有可导出的角色喜好资料");
    let mut canvas = Canvas::new(
        "雀魂角色喜好表",
        "樱花在左 · 紫色礼物居中 · 竹林在右 · 金色边框表示限定角色",
    );
    canvas.text(24, 104, "樱花", 20, "#16826e");
    canvas.text(900, 104, "竹林", 20, "#16826e");
    let mut y = 120;
    for (group_index, (_, (gift, characters))) in groups.into_iter().enumerate() {
        if group_index > 0 {
            canvas.body.push_str(&format!(
                "<path d='M24 {} H1576' stroke='#d0dbe3' stroke-width='1'/>",
                y - 12
            ));
        }
        let sides = [
            characters
                .iter()
                .copied()
                .filter(|c| c.sex == 1)
                .collect::<Vec<_>>(),
            characters
                .iter()
                .copied()
                .filter(|c| c.sex == 2)
                .collect::<Vec<_>>(),
        ];
        let rows = sides
            .iter()
            .map(|s| s.len().div_ceil(7))
            .max()
            .unwrap_or(1)
            .max(1) as u32;
        let height = rows * 110 + 24;
        // Use the same artwork size and label baseline as the character cards.
        let gift_top = y + (rows - 1) * 55;
        canvas.image(gift.icon.clone(), 762, gift_top + 4, 76)?;
        canvas.body.push_str(&format!(
            "<text x='800' y='{}' text-anchor='middle' font-size='14' fill='#213547'>{}</text>",
            gift_top + 95,
            escape(&gift.name)
        ));
        for (side, entries) in sides.iter().enumerate() {
            let left = if side == 0 { 24 } else { 900 };
            let side_rows = entries.len().div_ceil(7) as u32;
            let side_top = y + (rows - side_rows) * 55;
            for (i, c) in entries.iter().enumerate() {
                let x = left + i as u32 % 7 * 96;
                let top = side_top + i as u32 / 7 * 110;
                canvas.panel(x, top, 88, 102, c.limited);
                canvas.image(c.thumbnail(), x + 6, top + 4, 76)?;
                canvas.text(x + 5, top + 95, &short(&c.name, 6), 13, "#213547");
            }
        }
        y += height;
    }
    canvas.save(y + 16, path)
}

pub(super) fn owned(
    library: &Library,
    account: &AccountSnapshot,
    options: &OwnedOptions,
    path: &Path,
) -> Result<()> {
    ensure!(
        options.characters || options.skins || options.decorations,
        "请选择导出的收藏类型"
    );
    type Entry = (String, String, Option<PathBuf>, bool);
    let mut groups = Vec::<(String, Vec<Entry>)>::new();
    if options.characters {
        groups.push((
            "角色".into(),
            library
                .characters
                .iter()
                .filter(|c| account.character(&c.id).owned)
                .map(|c| {
                    (
                        c.name.clone(),
                        if c.sex == 1 { "樱花" } else { "竹林" }.into(),
                        c.thumbnail(),
                        c.limited,
                    )
                })
                .collect(),
        ));
    }
    if options.skins {
        groups.push((
            "皮肤".into(),
            library
                .characters
                .iter()
                .flat_map(|c| {
                    c.skins
                        .iter()
                        .filter(|s| account.owns_skin(s))
                        .map(move |s| {
                            (
                                s.display_name(&c.name).into(),
                                c.name.clone(),
                                s.thumbnail(),
                                c.limited,
                            )
                        })
                })
                .collect(),
        ));
    }
    if options.decorations {
        let mut decorations = BTreeMap::<&str, Vec<Entry>>::new();
        for d in library
            .decorations
            .iter()
            .filter(|d| account.owns_decoration(d))
        {
            decorations
                .entry(crate::text::decoration_name(&d.category))
                .or_default()
                .push((d.name.clone(), String::new(), d.image.clone(), false));
        }
        groups.extend(
            decorations
                .into_iter()
                .map(|(name, cards)| (format!("装扮 · {name}"), cards)),
        );
    }
    let mut canvas = Canvas::new(
        &format!("{}的收藏", account.profile.nickname),
        &format!(
            "UID {} · 金色边框表示限定角色{}",
            account.profile.uid,
            if account.is_demo {
                " · 调试预览"
            } else {
                ""
            }
        ),
    );
    let mut y = 106;
    for (label, entries) in groups {
        if entries.is_empty() {
            continue;
        }
        canvas.text(
            24,
            y + 22,
            &format!("{label}  ·  {}", entries.len()),
            21,
            "#16826e",
        );
        y += 38;
        for (i, (name, subtitle, image, limited)) in entries.iter().enumerate() {
            let x = 24 + i as u32 % 10 * 156;
            let top = y + i as u32 / 10 * 150;
            canvas.panel(x, top, 146, 140, *limited);
            canvas.image(image.clone(), x + 25, top + 5, 96)?;
            canvas.text(x + 7, top + 116, &short(name, 10), 13, "#213547");
            canvas.text(x + 7, top + 133, &short(subtitle, 11), 12, "#617384");
        }
        y += entries.len().div_ceil(10) as u32 * 150 + 14;
    }
    canvas.save(y + 16, path)
}
