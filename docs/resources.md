# 雀魂本地资源结构与解析原理

本文说明如何从一份雀魂 Windows 本地安装中定位资源、读取 Unity 对象、恢复配置与中文名称，并将素材对应到角色、皮肤和物品。后续开发者可按这条流程复现解析过程。

文档核对日期：2026-10-19。主要分析样本为 Steam Windows 客户端 `4.0.35`、Unity `2022.3.62f2c1`。文中的路径与样例来自该安装；处理其他版本时，应重新发现包名、对象引用并核对数量。

当前解析实现位于 `crates/majsoul-assets`，供[主应用](../README.md)使用。本文讲原始资源及解析方法；应用操作和 CLI 参数见项目 README，导出目录与 JSON 协议见 [素材导出格式](asset-export-format.md)。

## 目录

1. [本地安装里有什么](#1-本地安装里有什么)
2. [从逻辑路径定位到包内对象](#2-从逻辑路径定位到包内对象)
3. [恢复配置表和中文名称](#3-恢复配置表和中文名称)
4. [将资源对应到实际素材](#4-将资源对应到实际素材)
5. [将包内对象转换为可用素材](#5-将包内对象转换为可用素材)
6. [组织一个可持续使用的本地解析器](#6-组织一个可持续使用的本地解析器)
7. [本地以外的补充来源](#7-本地以外的补充来源)
8. [附录：实现入口与验证依据](#附录实现入口与验证依据)

## 1. 本地安装里有什么

### 1.1 安装目录与文件职责

主要素材已包含在本地安装中，可直接离线读取。

```text
MahjongSoul/
├─ GameAssembly.dll
└─ Jantama_MahjongSoul_Data/
   ├─ resources.assets
   ├─ resources.assets.resS
   ├─ il2cpp_data/Metadata/global-metadata.dat
   ├─ StreamingAssets/StandaloneWindows/
   │  ├─ AssetBundleConfig.json
   │  └─ *.majset
   ├─ ABM-Fold/StandaloneWindows/
   └─ ABM-CONFIG/StandaloneWindows/AssetBundleConfig.json
```

| 文件或目录 | 作用 |
| --- | --- |
| `StreamingAssets/StandaloneWindows` | 主要资源库，角色、装扮、动态皮肤及配置资源所在位置 |
| 主要资源库中的 `AssetBundleConfig.json` | 经编码的资源清单，将逻辑路径关联到包文件 |
| `*.majset` | Unity AssetBundle，保存图片、文本、模型、材质等对象 |
| `resources.assets` | 启动资源及识别真实 Unity 版本的入口 |
| `ABM-Fold`、`ABM-CONFIG` | 安装中的补充包及配置位置 |
| `GameAssembly.dll` 与 IL2CPP 元数据 | 用于研究客户端加载逻辑和配置编码 |

以用户选择的安装为边界定位资源。当前工具接受游戏 exe、安装根目录、Data、StreamingAssets 或 StandaloneWindows，向对应层级查找同时含资源清单和 `.majset` 的目录。初始化还需要配套的 `resources.assets`，因此应保留完整安装目录。

### 1.2 原始资源的三层关系

本地解析需要同时建立两条关系：一条用于找到对象，另一条用于理解对象代表什么。

```text
读取位置：资源清单 → 逻辑路径 → .majset → Unity 对象
业务身份：配置表 + 语言表 → 角色 / 皮肤 / 物品 → 逻辑路径或资源族
                                           ↓
                                名称、用途及可读取的素材
```

例如 `myassets/deco/character/jiutiao/full/full.png` 是包内逻辑路径。资源清单决定去哪个包找它；皮肤表和角色表决定它属于哪个角色、哪套皮肤。

### 1.3 已确认的素材范围

| 范围 | 本地内容 | 当前读取边界 |
| --- | --- | --- |
| 角色与静态皮肤 | 立绘、头像及客户端辅助图片 | 可读取图片并关联正式角色、皮肤和剧情形象 |
| 道具与礼物 | 物品配置、图集、命名 Sprite | 可还原独立图标，包括许愿石、粉尘、宝玉、福袋、匣子和礼物 |
| 装扮 | 图标、纹理、部分模型和场景资源 | 可读取图标并关联名称与资源族；完整表现另需模型、材质及场景逻辑 |
| 插画、称号、段位 | 图片及相关配置 | 可按配置关联，区分主图、缩略图、语言及共享徽章 |
| 动态皮肤 | Spine 骨架、atlas、纹理、分层信息和部分背景 | 可播放骨架、合成多层及恢复部分静态背景，尚非完整 Unity 表现 |
| 音乐与音效 | 音频路径清单及名称、物品、表现关联 | 已查明引用关系；当前工具未实现音频解码、试听或音频导出 |
| 其他资源 | 表情、字体、登录素材、传记背景及特效原料 | 作为辅助分析资源保留，读取能力视对象类型而定 |

验证分为资源存在、对象读取、业务关联和表现还原四个层次，分别记录结果。安装中还保留默认形象、测试条目、NPC、旧资源和地区变体，建立玩家收藏目录时应按业务属性筛选。

## 2. 从逻辑路径定位到包内对象

### 2.1 `.majset` 与 Unity 版本

本地抽查的 `.majset` 文件头为 `UnityFS\0`，是使用自定义扩展名的 Unity AssetBundle。UnityPy 和当前 Rust Unity 读取库都可直接解析这些包。包内配置 TextAsset 的载荷另有编码，见第 3 章。

部分包头的 Unity 版本为 `0.0.0`。应先解析同一安装的 `resources.assets`，取得真实版本，再作为包读取器的版本覆盖或回退值。把本次样本的 Unity 版本写死，可能在客户端更新后错误解释序列化结构。

包的外部文件名经过混淆，资源类别与读取位置应通过清单和包内引用确定。

### 2.2 解码资源清单

主要资源库的 `AssetBundleConfig.json` 按第 3.1 节的配方解码后，包含以下关键结构：

- `BundlesInfo`：以包文件名为键的对象。
- `AssetsInfo`：按目录嵌套的资源树。
- 资源树中的 `/f/`：文件列表，每项形如 `<文件名>_!_<包序号>`。

包序号引用 `BundlesInfo` 键的原始插入顺序，解析时必须保留该顺序。Rust 实现因此启用了 `serde_json` 的 `preserve_order`。文件名应从最后一个 `_!_` 分隔位置拆开，并校验包序号是否越界。

```text
保留顺序读取 BundlesInfo → 包名数组
递归 AssetsInfo：
  普通键：追加目录并继续遍历
  /f/：拆出文件名与包序号
       得到「完整逻辑路径 → 包名」
```

初始化根据这张映射记录路径、包大小和修改时间，图片解码延后到实际读取时进行。当前 schema 2 索引中的 `file=0`、`id=0` 可以是占位值，真实对象引用在打开目标包后解析。

### 2.3 从路径解析直接对象引用

打开目标包后，读取 AssetBundle 对象（class 142）的 `container` 表，以逻辑路径匹配 `entry.key`，再用 `entry.asset` 的文件引用和 path ID 定位真实对象。

| 常用 Unity 类型 | class ID | 用途 |
| --- | ---: | --- |
| Texture2D | 28 | 图片纹理、图集页面 |
| TextAsset | 49 | 配置载荷、atlas、骨架及其他文本或二进制内容 |
| Font | 128 | 嵌入字体 |
| AssetBundle | 142 | 逻辑路径到对象的直接引用 |
| Sprite | 213 | 独立精灵或图集中的裁切对象 |

对象身份由序列化文件身份和 path ID 共同确定。path ID 是 64 位整数；需要传给 JavaScript 时，应以字符串表达，避免精度损失。

当前 Unity 读取库的便捷 `object.container()` 元数据可能把依赖对象标记为引用者的容器路径。骨架 TextAsset 等资源应优先按 AssetBundle 的直接引用查找。preload 表用于依赖加载。当前直接读取路径遇到尚未支持的外部序列化文件引用时会报错。

### 2.4 路径规范化的范围

资源清单和当前关联实现使用小写逻辑路径，并补齐必要的 `myassets/` 前缀。原始引用仍有分析价值，尤其是错误诊断和语言变体核对。

物品图标匹配允许在 `/pic/` 下识别 `chs`、`common` 等语言层级，例如配置中的 `pic/name.jpg` 对应本地的 `pic/common/name.jpg`。规范化规则限定在这些图标位置，其他目录层级和后缀保留。业务身份仍由配置外键和明确资源引用确定。

## 3. 恢复配置表和中文名称

### 3.1 本地载荷解码配方

本次安装的资源清单及相关 Lua TextAsset 使用相同的外层编码：

| 步骤 | 参数 |
| --- | --- |
| 1. Base64 解码 | 标准 Base64，忽略载荷中的 ASCII 空白 |
| 2. AES 解密 | AES-128-CBC |
| Key | ASCII `wrelupqezdfrqdsd`，16 字节 |
| IV | ASCII `sfaqweertfvfdxad`，16 字节 |
| 3. 去除填充 | PKCS#7 |
| 4. 解释内容 | 清单为 UTF-8 JSON；相关配置为 UTF-8 Lua 数据 |

这些参数来自安装文件中的客户端代码。实现见 [names.rs](../crates/majsoul-assets/src/core/names.rs) 的 `decode`。解码完成后，以 JSON 或 Lua 结构校验确认结果。

该配方已在本文样本中验证。新版本出现 AES 填充失败或输出结构异常时，应报告编码不兼容并重新核对参数。

### 3.2 配置入口、分块与语言文件

所需文件通过资源清单按逻辑路径发现：

```text
luabyte/lua/excels/data/<表域>/<表名>.lua.bytes
luabyte/lua/excels/data/<表域>/<表名>_bN.lua.bytes
luabyte/lua/excels/langs/<表域>_<表名>_<字段名>_chs.lua.bytes
```

完整表由入口、相关 `_bN` 分块和语言文件共同恢复：入口描述结构与默认值，分块承载行数据，语言文件将行中的索引还原为中文。

当前解析器读取的主要表包括：

| 表域 | 表 |
| --- | --- |
| `item_definition` | `character`、`skin`、`item`、`view`、`title`、`loading_image` |
| `spot` | `skin_spot`、`character_spot` |
| `audio` | `bgm`、`audio` |
| `level_definition` | `level_definition` |

名称字段主要使用 `name_chs`，并按需要读取 `full_name_chs`。增加其他语言、描述文字或业务表时，需要扩展相应的载荷选择与语言关联。

### 3.3 恢复压缩行的规则

这些 Lua 文件保存客户端生成的数据。当前解析器直接读取数据字面量；字段编码和压缩语义已与同安装的 ExcelTool 脚本核对。

1. 从入口恢复「字段名 → 编码位置」以及默认值数组。
2. 正位置表示普通值，1000 段的位置用于字符串，负位置表示语言索引；实际列位置为 `abs(编码位置) % 1000`，按 1 起算。位置合法范围需要校验。
3. 每一行先复制默认值。行中的 `false` 是沿用默认值的标记。
4. Dmap 行中的 `e[start*1000+end]` 表示省略位置区间，包含两端。应将后续显式数据写到跳过区间之后，保留区间内默认值。
5. 对负位置字段，用字段对应语言数组将索引转换为文字；语言索引从 1 开始。
6. 按字段名生成恢复后的记录，再按业务 ID 建立外键关系。

实现见 [lua.rs](../crates/majsoul-assets/src/core/lua.rs)。列位置按当前入口恢复；未知编码、超出布局的行或缺少必要结构时应报错。

### 3.4 更新适配的边界

通过清单发现路径、从入口恢复布局，可以适应包重命名、分块数量增加、行数变化和字段位置变化。加密算法、表格式、字段语义或业务外键关系发生变化时，仍需要调整解析器。

每次更新后按当前结构重新发现并校验。名称关联失败的条目保留原始路径和未知状态，供浏览和进一步核对。

## 4. 将资源对应到实际素材

业务关联以配置 ID、明确外键和资源引用为依据。目录名的拼音、数字前缀和视觉相似度可辅助研究，最终关系须落实到配置或对象引用。

### 4.1 角色与静态皮肤

主要关系是 `skin.character_id → character.id`。角色表提供角色名和角色属性；皮肤表提供皮肤身份、名称及资源选择信息。

同安装 SkinTool 的选择规则为：

```text
GetSkinInfo(id)：spot.skin_spot 优先，否则 item_definition.skin
GetCharacterInfo(id)：spot.character_spot 优先，否则 item_definition.character
静态图片目录：
  skin_version == 1 → skin.path
  其他情况         → myassets/deco/character/<完整 skin.id>
```

皮肤使用完整 ID，通过 character_id 关联角色。比如 `400501` 是相原舞初始形象、`400502` 是契约形象，`402601/402602` 对应雏桃；它们走完整皮肤 ID 的数字目录分支。`ailisha_99`、`erzhigonghua_99` 等剧情目录则由 `spot.skin_spot` 的路径记录关联。

同一皮肤目录中的图片用途不同：

| 原始子目录 | 用途 |
| --- | --- |
| `full` | 静态完整立绘 |
| `bighead` | 大头像 |
| `smallhead` | 小头像 |
| `face`、`half`、`waitingroom` 等 | 客户端其他表现或辅助图片 |

初始与契约形象通过角色的 `init_skin`、`full_fetter_skin` 判断。角色展示顺序使用配置 `sort`，再以角色 ID 稳定排序；缺失 sort 的条目放末尾。正式角色的剧情形象沿用正式角色顺序，剧情 NPC 可使用 `spot.character_spot.sort`。

角色类型直接读取 `sex`（样本中 1 女、2 男）、`limited`、`collaboration`、`treasure_sp`。未知值保留为空；`collaboration > 0` 用作联动标记，原始数值与 `treasure_sp` 一并保留。当前卡池和获取途径需要额外业务资料。

### 4.2 道具、礼物及其图标

物品资料来自 `item_definition.item`，主要使用 `id`、`name_chs`、`icon`、`category`、`type` 及到期信息。

物品目录读取 `item_definition.item` 中 category=1（通用道具）、2（礼物）、3（旧福袋）、6（活动道具）的记录。养成材料通过角色的 `star_5_material` 引用识别，券与卷轴、福袋与宝盒另行归类。物品定义独立于图片：部分旧活动条目引用的图片已不在本地安装中，仍可保留名称与 ID。

礼物 type=1–8 分别为曲奇、游戏机、美术品、酒、宝石、玩偶、同人志、裙子；type=99 为特殊礼物。`iargs[0]` 是基础赠礼好感值，八类常规礼物中的 200、1000、5000 对应绿色、蓝色、紫色。蓝色和紫色礼物均有锁定版本，使用不同业务 ID，图标名带 `_limit`。库存按 ID 分别记录；契约按 `star_5_material` 的等价 ID 组合求和。

图标通过 `extendres/items/` 下的本地图集配置和命名 Sprite 定位。缺少图集的条目保留定义，消费端可显示占位图。解析与分类见 [core/items.rs](../crates/majsoul-assets/src/core/items.rs)。

主应用的道具展示规则位于 [inventory-filter.json](../src/assets/inventory-filter.json)，通过 `include_str!` 编译到可执行文件。`excluded_groups` 隐藏整类道具，`excluded_items` 按精确 ID 隐藏指定条目，值为维护时参考的名称。当前隐藏活动道具、其他道具、试炼券和红牛。规则在加载素材库时生效，修改后重新编译即可应用到已有素材库。原始物品定义与账号库存保留，契约计算继续按配置的等价 ID 组求和。

`item.icon` 可以表示图集中的虚拟单图路径。例如 `extendRes/items/biscuit0.jpg` 需要通过图集配置找到页面和 Sprite，具体读取方法见第 5.2 节。

带锁图标对应的独立物品 ID 在原始解析中保留，导出时按选项筛选。当前安装依据图标中的 `_limit` 或 lock 标记识别变体；更新后应随图标配置核对这条规则。

到期信息同样属于业务规则：当前按 `item_expire` 和“已过期”标记识别，日期按客户端 UTC+8 解释；未知日期格式的条目保留。

### 4.3 喜好礼物与契约材料

喜好关系是 `character.favorite → item.type`，并筛选礼物 category=2。同一礼物类型中可以包含不同品质、普通及限时物品。

契约材料来自 `character.star_5_material`。逗号分隔需求组，组内格式为：

```text
物品ID|物品ID-数量
```

竖线两侧的物品可混用，以组内持有数量之和满足这一份需求。同安装 `UI_Visit.RenderQy` 使用该求和结果与需求数量比较。

一姬样例：

```text
302010-10,302011-10,303113|303013-5,302002-10,302004-100
```

其中 `303113` 与 `303013` 都是「香喷喷曲奇」的物品身份，该组需求总量为 5。原始解析保留两个 ID、各自名称和图标，以及这一组的总数量。角色的 `can_marry`、`star_5_cost` 等配置一并保留；币种和好感条件通过客户端业务逻辑解释，达成情况结合账号数据判断。

### 4.4 装扮与实际表现资源

装扮通常使用 `item.id = view.id` 关联。item 提供名称和图标，view 的 `res_name`、`sargs` 等字段关联表现资源族。一个资源族可以被多个条目共享；关联结果保留各个身份及其依据。

以下路径均相对 `myassets/`：

| 分类 | 主要路径 | 关联与读取要点 |
| --- | --- | --- |
| 和牌特效 | `deco/effect_hupai` | item/view → res_name；分别识别图标、散图和表现依赖 |
| 立直特效 | `deco/effect_lizhi` | 同上；目录使用 lizhi，部分资源名使用 liqi，按配置保留实际拼写 |
| 鸣牌指示 | `deco/effect_mingpaizhishi` | item/view 定位资源族 |
| 立直棒 | `deco/lizhibang` | 区分物品图标、模型和模型纹理 |
| 桌布 | `deco/tablecloth` | pic 通常是物品图标，3d/texture 为表现纹理；地区变体按配置关联 |
| 牌背 | `deco/mjpai` | 图标、预览图及模型纹理分别识别 |
| 牌面 | `deco/mjpface` | 独立于牌背，按配置区分可装备条目与特殊模式资源 |
| 头像框 | `deco/head_frame` | 区分物品图标、透明框图和动态表现 |
| 手的样式 | `deco/hand` | 商品手模走 item/view；默认手还需 character.hand → view.id → res_name |
| 大厅背景图标 | `deco/lobby_background` | 与背景共用业务身份，图标与背景大图分别定位 |
| 大厅背景主图 | `extendres/background/lobby` | 结合 item/view 的资源名和参数定位；部分背景还有场景效果 |
| 音乐图标 | `deco/bgm`、`deco/lizhi_bgm` | 一个图标可被多首音乐共享，曲目关系见第 4.6 节 |

例如默认人手 `hand_human` 由多个角色共享，`headframe_douya` 同时被普通和测试条目引用，关联结果都应保留这些共享关系。

对于配置未直接引用的旧目录，可建立有证据的别名。当前实现只比较同分类、同语言 pic 图标的尺寸和解码 RGBA 像素；唯一匹配时才关联身份。这类证据用于关联图标身份；模型、动画和特效仍按各自引用核对。详见附录 B。

### 4.5 插画、称号与段位

| 类型 | 配置关系 | 注意事项 |
| --- | --- | --- |
| 插画 | `loading_image.img_path / thumb_path → 图片`；`unlock_items → item.id → 名称` | 主图和缩略图分开；本地主要路径为 `deco/loading_cg` |
| 称号 | `title.id/name_chs/icon/icon_item → deco/title` | 区分称号本体、物品展示图及其语言变体 |
| 段位 | `level_definition.level_definition.primary_icon → 图标` | 按 primary_icon 去重为大段位徽章，保留四麻/三麻差异 |

业务记录与图片可以是多对一关系，统计时应分别记录业务条目数和独立图像数。

### 4.6 音乐与音效

音乐图标和音频本体分别关联，曲目数按音乐配置记录统计。

- 大厅／对局 BGM：读取 `audio.bgm` 的名称、type、path、unlock_item。`path=music/lobby.mp3` 对应 `myassets/audio/music/lobby.mp3`。
- 立直音乐：读取 item 中 category=5、type=4 的条目，名称取 item.name_chs，实际音乐路径来自 sargs。
- 特效音效：通过 `view.audio_id → audio.audio.id → path` 查找。

本地示例：305025「真剑胜负」引用 `music/liqi1.mp3`；30505001「处变不惊」引用 `music/liqi_23winer.mp3`。路径保留配置中的实际拼写。

清单中的 `.mp3` 后缀描述逻辑资源。音频解码需要读取实际 Unity 音频对象并处理其编码；当前工具完成了关系提取和音乐图标输出。

### 4.7 其他辅助资源

| 路径 | 含义 |
| --- | --- |
| `deco/emo` | 表情 |
| `deco/cutin` | 动态皮肤的特殊和牌表现原料 |
| `deco/effect_click` | 大厅背景点击特效 |
| `deco/lobby_background_effect` | 大厅背景附加特效 |
| `extendres/background/story` | 传记背景，与大厅背景分开 |
| `fonts` | 字体相关对象，可能同时含字图集与真实 Font |
| `scenes/entrance` | 登录素材，包括地区语言版本 Logo |

这些目录提供资源用途线索，具体名称和渲染依赖按对象进一步核对。

## 5. 将包内对象转换为可用素材

### 5.1 普通纹理与 Sprite

按逻辑路径找到目标 Unity 对象后，解码纹理压缩，再生成普通 PNG/JPEG 图片。

对独立角色立绘，优先读取对应的直接 Texture2D，保留完整画布；只存在 Sprite 时按 Sprite 的裁切和图集关系解码。当前读取库返回的 Texture2D 像素需要垂直翻转，Sprite 保持解码器输出方向。该方向规则属于当前读取库的输出约定，换库时需重新用真实样本验证。

同一路径在不同对象类型或引用中重复出现时，通过对象类型、尺寸、透明区域和用途选择所需画布。

### 5.2 从物品图集恢复独立图标

以 `extendRes/items/biscuit0.jpg` 为例，读取链路为：

```text
item.icon → 图标名 biscuit0
          → 所在目录的 chs/common 图集配置
          → 配置中的页号
          → 对应 atlas 页逻辑路径及资源包
          → 名为 biscuit0 的 Sprite
          → 独立 PNG
```

图集配置通常位于：

```text
myassets/extendres/items/<chs或common>/atlas_extendres_items_<语言>_config.json
```

配置将图标名映射到页号。将 `_config.json` 替换为 `_<页号>.png` 后得到页面逻辑路径，再通过资源清单找实际包。当前实现优先检查 chs，未包含该图标时再检查 common。

配置和图集页面分别通过清单定位。页面中的目标图标按 Sprite 名称解码，裁切区域由 Sprite 元数据确定。解析结果和页面解码均可复用缓存，见 [item_icons.rs](../crates/majsoul-assets/src/core/item_icons.rs)。

### 5.3 嵌入字体

字体逻辑路径可能同时暴露字图集 Texture2D 和 Unity Font。提取字体时定位 class 128 对象，读取嵌入的 TTF/OTF 数据。

当前查看器将读取结果交给 WebView FontFace 预览。缺少字形时系统可能使用回退字体，核对字体效果时应同时检查字形覆盖。

### 5.4 单个 Spine 骨架的依赖收集

本地动态皮肤包含骨架动画与图集数据，典型文件为：

- `*.skel.txt`：二进制骨架载荷，读取时保留原始字节。
- `*.json`：部分皮肤使用 JSON 骨架。
- `*.atlas.txt`：图集页、区域坐标及旋转等布局。
- PNG 纹理页：根据 atlas 页名查找，可以位于其他资源包。

应先读取 atlas，保留其页名和区域布局，再定位配套骨架和每一张纹理。骨架解析器必须与实际版本兼容；当前样本主要为 Spine `4.2.08`，工具明确支持 4.2 系列，并使用本地随附的 Spine Player 4.2.120 播放。

atlas 的声明尺寸可能小于 Unity 纹理的实际画布。样例中存在 atlas 3937×3937、实际纹理 4096×4096 的情况，读取时保留 atlas 声明和原始纹理布局。透明度按 atlas 的 pma 标记处理；当前实现明确 `pma:true` 时使用预乘透明度，否则按 straight alpha。

运行时需要完整的骨架、atlas 和所有引用纹理。缺失纹理时报告准确路径。这里恢复的是 runtime 所需资源，Spine 编辑器原工程需要另行取得。

### 5.5 多层合成、动画同步与背景

编号目录代表同一皮肤的图层，图层在共同坐标系中叠加；实际采用哪些目录由分层配置决定。

图层选择顺序为：优先使用 `<皮肤ID>_info.json` 中明确的 layers；旧结构则按编号 `character.prefab` 的顺序；没有分层配置时尝试皮肤根目录。客户端部分布局只使用目录 2，是正常结构。

当前合成器让多个骨架共用画布、坐标系和相机，按配置顺序绘制。主层的动画名和时间用于同步其他层；缺少对应动作的层使用 idle 或初始姿态，并保留提示。客户端的其他交互状态需要结合状态脚本继续恢复。

部分背景通过 Unity 场景引用，例如 `40010803/texture/jiantongying_001.png` 由 ExportRoot 中的 MeshRenderer 经材质 `jiantongying_010.mat` 的 `_MainTex` 引用。应沿 GameObject/Transform 层级读取位置和缩放，再应用材质 UV、颜色，得到背景的绘制位置和范围。

当前背景恢复支持 bg/background 子树中的轴对齐 Unity Quad。任意网格、旋转、自定义 shader 扭曲、粒子和场景遮罩尚未完整重建。图层读取失败时保留可用层并附具体警告；全部失败则返回场景读取错误。

## 6. 组织一个可持续使用的本地解析器

### 6.1 推荐处理流程与当前实现

| 阶段 | 输入与产物 | 是否需要解码图片 |
| --- | --- | --- |
| 定位安装 | 用户路径 → 资源根目录及配套 Unity 版本 | 否 |
| 建立路径索引 | 解码清单 → 逻辑路径、包名、文件状态 | 否 |
| 恢复配置 | 入口、分块、语言载荷 → 按字段名组织的配置记录 | 否 |
| 建立业务关系 | 配置 ID、路径和资源族 → 角色、皮肤、物品及关联依据 | 通常否；旧图标别名像素核对是例外 |
| 按需读取 | 指定资源 → 解码图像、字体或骨架依赖 | 只处理实际请求的内容 |
| 生成交换素材包 | 业务关系和所选内容 → 普通素材与结构化 JSON | 详见导出格式文档 |

当前工具的 `import` 负责初始化及关联资料，`export` 生成独立素材包，两者和 GUI 共用解析逻辑。内部索引服务于 Unity 资源读取；Gallery 通过导出的 JSON 和普通素材文件取得可展示内容。

### 6.2 持久缓存与更新检查

当前便携数据都保存在应用旁：

```text
majsoul-assetviewer-data/
├─ settings.json
├─ indexes/
├─ names/
└─ cache/
```

| 数据 | 用途与失效条件 |
| --- | --- |
| settings | 持久保存已选择来源等设置 |
| 路径索引 | 保存资源映射、Unity 版本、包大小/修改时间和清单摘要；正常打开复用，明确重建时重新生成 |
| 名称缓存 | 保存恢复的配置及相关资源映射；签名包含解析器版本、来源、Unity 版本、清单及相关配置包状态 |
| 角色资料和别名缓存 | 记录由配置生成的资料及已验证图标别名；内容或源资源未变化时复用 |
| 预览 cache | 保存图片、字体、骨架等转换结果，可按需重新生成；独立交付使用正式导出目录 |

Names::load 每次解码清单以发现路径并构造签名；名称缓存命中后复用已恢复的表，跳过 Lua 包解析。

资源更新检查使用清单 SHA-256、包集合及文件大小/修改时间。若内容替换同时保留了包大小和修改时间，这类变化会超出包状态检测范围。旧索引缺少清单摘要时需重建以建立基线；发现更新后先重建索引，再继续读取。

### 6.3 批量读取的任务组织

批量处理时，应先确定所选资源、目标路径和可发现的依赖，再集中处理任务引用的各个资源包。

当前图片导出按大分类分阶段规划，提前为物品定位实际图集页，随后跨条目按包处理，最多同时处理两张图片。已有增量目标跳过依赖解析和解码。动态皮肤仍按场景处理，角色图标依赖最后补齐。跨阶段依赖可能再次访问同一包，共享缓存用于减少重复载入。

解析后的包使用共享 LRU 缓存，以约 512 MiB 的估算保留预算控制容量，没有固定包数上限；正在使用的包保持共享引用。进程总内存还包括活动任务、展开纹理和临时编码缓冲。

是否值得进一步扩大缓存，取决于同包的再次访问距离和解码后体积。一个只有一张图集的包可能被许多物品共享；反之，含大量资源的包也可能只读取一次。

### 6.4 未知关系与结果验证

未匹配条目保留原始路径和未知状态；同资源关联多个业务 ID 时保留全部关系；缺包、缺对象和缺依赖分别报告。材料格式或表结构变化时保留原始表达式，避免产生看似合理的错误资料。

建议沿完整链路验证：

1. 清单中的目标路径能找到包，直接引用对象类型符合预期。
2. skin 能关联正确角色，item/view 等外键有明确来源，共享和剧情条目按各自身份保存。
3. 图片尺寸、方向、透明画布及图集裁切符合实际用途。
4. 骨架所有页、图层和背景引用完整，缺失表现有可见警告。
5. 相同来源再次运行能复用索引与缓存；源包变化时旧索引会被识别为不适用。

资源库描述安装内容；账号真实持有量、角色好感、契约状态和装备状态需要独立的数据来源。

## 7. 本地以外的补充来源

### 7.1 公开客户端配置快照

已有研究使用 [Avenshy/MajsoulData 的固定快照](https://github.com/Avenshy/MajsoulData/tree/2677b55ad27d) 辅助确认表结构和关联方式，相关入口为 [配置表目录](https://github.com/Avenshy/MajsoulData/tree/2677b55ad27d/data/tables/item_definition) 与 [提取实现](https://github.com/Avenshy/MajsoulData/blob/2677b55ad27d/src/pipeline.py)。

该快照记录的客户端为 `4.0.47`、`chs_t`、ASTC，本地样本为 Windows `4.0.35`。快照用于对照字段、路径体系和读取算法；本地名称及覆盖率以同安装配置核对。

当前运行流程直接解析本地配置，公开快照作为单独的研究资料保存。

### 7.2 网页版本清单与配置路线

此前对英文服网页客户端的研究保留如下可复用路线：

```text
游戏网页入口 → version.json → 版本化配置与资源清单
                              ├─ 配置定义 config.proto
                              ├─ 表载荷 lqc.lqbin
                              └─ 按清单发现实际资源路径
```

历史入口为 `https://game.maj-soul.com/1/`。当时曾读取 `version.json`、版本化 `config.json` 和 `config.proto`，并用后者描述的 ConfigTables 结构解析 lqc 表数据。这条路线已验证业务记录和资源引用的读取，图片下载及还原链路留待后续开发。

旧网页配置使用过 `extendRes/charactor/yiji` 路径，其对应素材应沿网页资源清单定位。本地 `deco/character/...` 使用独立的目录体系。后续下载器需从清单逐级发现各项资源的版本前缀和 URL。

远程入口的可用性在开发下载器时重新检查。补充本地缺失素材时，可设计独立来源适配器，并记录来源、版本、平台和语言。账号协议、认证与玩家收藏读取由独立流程处理。

## 附录：实现入口与验证依据

### A. 当前实现入口

以下解析入口均位于 `crates/majsoul-assets/src/`：

| 文件 | 职责 |
| --- | --- |
| [core/index.rs](../crates/majsoul-assets/src/core/index.rs) | 来源定位、索引、设置和资源更新检查 |
| [core/names.rs](../crates/majsoul-assets/src/core/names.rs) | 清单与载荷解码、配置加载、名称和路径关联 |
| [core/lua.rs](../crates/majsoul-assets/src/core/lua.rs) | Lua 数据字面量、字段布局及压缩行恢复 |
| [core/characters.rs](../crates/majsoul-assets/src/core/characters.rs) | 角色属性、喜好和契约资料 |
| [core/business.rs](../crates/majsoul-assets/src/core/business.rs) | 图集中的道具、礼物与段位业务条目 |
| [core/aliases.rs](../crates/majsoul-assets/src/core/aliases.rs) | 旧资源图标的像素证据与别名缓存 |
| [core/preview.rs](../crates/majsoul-assets/src/core/preview.rs)、[core/item_icons.rs](../crates/majsoul-assets/src/core/item_icons.rs) | 普通图片、字体及图集 Sprite 读取 |
| [core/spine.rs](../crates/majsoul-assets/src/core/spine.rs)、[core/spine_background.rs](../crates/majsoul-assets/src/core/spine_background.rs) | 骨架、图层、纹理与静态场景背景 |
| [core/bundles.rs](../crates/majsoul-assets/src/core/bundles.rs) | 解析后包和图集配置的共享缓存 |
| [core/session.rs](../crates/majsoul-assets/src/core/session.rs) | GUI 与 CLI 共用的资源库载入流程 |
| [core/export.rs](../crates/majsoul-assets/src/core/export.rs) | 交换素材包规划和导出，协议另见导出格式文档 |

当前 Rust 实现固定 Unity 读取库版本，Python 原型曾使用 UnityPy 1.25.4。现行解析流程沿上述入口核对。

### B. 旧目录别名的已验证例子

以下关联来自同分类、同语言图标的解码像素核对。路径相对 `myassets/deco/`：

| 旧目录 | 配置引用目录 | 物品 ID | 名称 |
| --- | --- | ---: | --- |
| `effect_hupai/25yh_ron` | `effect_hupai/ron_25yh` | 308046 | 和牌-谢幕的Just a way |
| `effect_lizhi/25yh_liqi_effect` | `effect_lizhi/effect_liqi_25yh` | 308047 | 立直-伊丽莎白 |
| `lizhibang/25yh_liqi` | `lizhibang/liqi_25yh` | 308048 | 立直棒-志村新八 |
| `tablecloth/25yh_table` | `tablecloth/tablecloth_25yh` | 308049 | 桌布-骑摩托的银时 |
| `mjpai/25yh_pai` | `mjpai/mjp_25yh` | 308050 | 牌背-Just a way |
| `head_frame/25yh_headframe` | `head_frame/headframe_25yh` | 308051 | 头像框-伊丽莎白框 |

头像框的像素证据仅在简体图上成立，繁体图保留独立结果。多个配置身份命中通用占位图时，结果保持为歧义匹配。内部像素摘要用于证据核对和缓存；导出素材使用业务 ID 命名。

### C. 加密参数的定位依据

本次研究通过同安装 `GameAssembly.dll` 和 `il2cpp_data/Metadata/global-metadata.dat` 确认 StringObfuscator 的静态初始化与 SetAesState。使用的工具为 [Il2CppDumper 6.7.46](https://github.com/Perfare/Il2CppDumper/releases/tag/v6.7.46)，随后核对初始化代码和实际载荷解码结果。

样本定位记录：SetAesState RVA `0x602000`，静态初始化 RVA `0x6020C0`；元数据中的 Key 字节位于 `0x627E38`，IV 位于 `0x5F85C8`。这些地址用于复核本次二进制，新版本需重新定位；运行时直接采用已确认的解码配方。

### D. 代表性验证样本

下表记录 2026-10-18 至 2026-10-19 的本地研究与实现验证范围：

| 样本 | 已验证内容 |
| --- | --- |
| 531 份配置载荷 | 使用相同配方恢复为配置数据；按载荷数量统计 |
| 本地 124 条角色资料涉及的 51 个物品 ID | 角色喜好及契约所需图标可解码为独立图像 |
| `400107/yiji_kxj` | 二进制骨架为 4.2.08，具有 idle、greeting、click 等动作 |
| `40010704` | 多层骨架可以在同一画布按顺序合成 |
| `409903/2/huayubai_lf` | 使用 JSON 骨架并可播放 |
| `40010803` | Spine 之外的静态背景可沿场景和材质引用恢复 |
| 早期全包解析与后续路径索引 | 分别统计包解析和路径引用，比较前需统一对象与去重口径 |

覆盖率按明确版本的配置目标逐项验收，记录资源存在、依赖完整、解码成功和表现恢复的结果；图片目录数、骨架文件数和可读包数分别统计。
