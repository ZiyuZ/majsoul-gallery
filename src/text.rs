//! Shared product vocabulary. Feature-specific explanations stay with their UI.
pub const APP_NAME: &str = "雀魂藏馆";
pub const COLLECTION: &str = "收藏";
pub const TRAINING: &str = "培养";
pub const ITEMS: &str = "道具";
pub const CHARACTERS: &str = "角色";
pub const SKINS: &str = "皮肤";
pub const DECORATIONS: &str = "装扮";
pub const TITLES: &str = "称号";
pub const SETTINGS: &str = "设置";
pub const EXPORT: &str = "导出图表与清单";
pub const LOGIN: &str = "登录账号";
pub const ACCOUNT_SYNC: &str = "登录与同步";
pub const SYNC_NOW: &str = "立即同步";
pub const LOGOUT: &str = "登出";
pub const OFFLINE: &str = "未登录";
pub const OPEN_PACKAGE: &str = "打开素材包";
pub const COLLAPSE_SIDEBAR: &str = "收起侧栏";
pub const EXPAND_SIDEBAR: &str = "展开侧栏";
pub const TOGGLE_THEME: &str = "切换主题";
pub const SEARCH_PLACEHOLDER: &str = "搜索名称、拼音、首字母或 ID…";

pub fn decoration_name(category: &str) -> &'static str {
    match crate::model::decoration_category(category) {
        "hand" => "手的样式",
        "frame" => "头像框",
        "table" => "桌布",
        "stick" => "立直棒",
        "tile-back" => "牌背",
        "tile-face" => "牌面",
        "win-effect" => "和牌特效",
        "riichi-effect" => "立直特效",
        "call-effect" => "鸣牌指示",
        "background" => "大厅背景",
        "bgm" => "音乐",
        _ => DECORATIONS,
    }
}

pub const ALL: &str = "全部";
pub const OWNED: &str = "已拥有";
pub const FAVORITE: &str = "星标";
pub const CONTRACTED: &str = "已契约";
pub const FULL_BOND: &str = "满好感";
pub const CONTRACT_READY: &str = "可契约";
pub const SAKURA: &str = "樱花";
pub const BAMBOO: &str = "竹林";
pub const STANDARD: &str = "常驻";
pub const LIMITED: &str = "限定";
pub const DEBUG_PREVIEW: &str = "调试预览";

pub const HOME: &str = "主页";
/// Friendly labels for standard Spine tracks; unknown artist-defined names remain intact.
pub fn animation_name(name: &str) -> String {
    name.split(['+', '_'])
        .map(|part| match part.to_ascii_lowercase().as_str() {
            "idle" => "待机".into(),
            "celebrate" => "庆祝".into(),
            "greeting" | "hello" => "问候".into(),
            "click" => "点击".into(),
            "touch" => "触摸".into(),
            "enter" | "in" => "登场".into(),
            "out" => "退场".into(),
            "loop" => "循环".into(),
            "animation" | "default" => "默认动作".into(),
            "dance" => "舞蹈".into(),
            "talk" => "说话".into(),
            _ => part.to_owned(),
        })
        .collect::<Vec<String>>()
        .join(" · ")
}
