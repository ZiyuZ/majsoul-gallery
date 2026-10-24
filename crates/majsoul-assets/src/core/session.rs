//! Shared library loading for the desktop and command-line interfaces.
use super::{Catalog, Library, names::Names, progress::Task};
use anyhow::Result;

pub struct Opened {
    pub names: Option<Names>,
    pub library: Library,
    pub catalog: Catalog,
    pub name_warning: Option<String>,
    pub named_groups: usize,
}

pub fn load(input: Option<&str>, task: &Task) -> Result<Opened> {
    task.update("读取索引", 0, 6, "");
    let library = Library::open(input)?;
    task.update("整理素材目录", 1, 6, "");
    let mut catalog = Catalog::build(&library);
    task.update("解析名称和配置表", 2, 6, "");
    let names = Names::load(&library);
    let (named_groups, name_warning) = match &names {
        Ok(names) => {
            super::spine::supplement_catalog(names, &mut catalog);
            let mut count = names.apply(&mut catalog);
            task.update("匹配旧资源名称（首次运行可能较久）", 3, 6, "");
            let aliases = super::aliases::apply(&library, &mut catalog);
            if let Ok(aliases) = &aliases {
                count += aliases;
            }
            task.update("关联角色、喜好与契约", 4, 6, "");
            let warning = names
                .apply_characters(&library, &mut catalog)
                .err()
                .map(|e| format!("角色礼物／契约资料未解析：{e:#}"))
                .or_else(|| {
                    aliases
                        .err()
                        .map(|e| format!("旧目录图标别名缓存失败：{e:#}"))
                });
            super::business::populate(names, &mut catalog);
            (count, warning)
        }
        Err(error) => (0, Some(format!("本地名称配置未载入：{error:#}"))),
    };
    task.update("整理收藏分类", 5, 6, "");
    catalog.organize();
    task.update("资源库已载入", 6, 6, "");
    Ok(Opened {
        names: names.ok(),
        library,
        catalog,
        name_warning,
        named_groups,
    })
}
