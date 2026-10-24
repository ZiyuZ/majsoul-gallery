# Gallery 素材包格式规范 v2

本文定义素材解析库生成的目录和 JSON 结构，供 Gallery 或其他素材消费端读取。使用方通过包内 JSON 和素材文件取得可展示内容。

核对日期：2026-10-19。本文描述当前 schema v2 的实际输出、字段差异及更新行为。原始素材的来源与解析原理见 [本地资源分析](resources.md)，应用使用和构建方法见 [项目 README](../README.md)。

## 目录

1. [素材包概览与读取流程](#1-素材包概览与读取流程)
2. [通用数据约定](#2-通用数据约定)
3. [根入口 manifest.json](#3-根入口-manifestjson)
4. [角色与皮肤](#4-角色与皮肤)
5. [道具、礼物及其他分类](#5-道具礼物及其他分类)
6. [动态皮肤场景](#6-动态皮肤场景)
7. [导出选项与目录更新语义](#7-导出选项与目录更新语义)
8. [消费端校验](#8-消费端校验)

## 1. 素材包概览与读取流程

### 1.1 包含哪些内容

素材包由结构化资料、PNG 图片和可选的 Spine 动画资源组成。静态图可直接展示，动态皮肤通过兼容的 Spine runtime 播放。

导出支持角色静态皮肤、动态皮肤、道具、礼物、装扮、插画、称号和段位。包内资料描述本地安装中的资源，玩家收藏状态由消费端另行关联。

装扮导出展示图标，头像框另保留框图；音乐保存名称与图标关系。动态皮肤包含骨架、图集、纹理及已恢复的静态背景，播放器由消费端提供。当前尚未实现音频、完整 3D 表现及 Unity shader、粒子和遮罩导出。

### 1.2 目录结构

独立导出时，选择父目录后会在其下生成 `majsoul-assets/`，也可以直接选择已有的同名目录。主应用将同样的素材包结构直接存放在 `majsoul-gallery-data/materials/` 中。目录内的 `manifest.json` 是素材包入口。

```text
majsoul-assets/
├─ manifest.json
├─ characters/
│  └─ 20000108/
│     ├─ character.json
│     └─ skins/
│        └─ 40010803/
│           ├─ skin.json
│           ├─ full.png
│           ├─ portrait.png
│           ├─ avatar.png
│           └─ spine/
│              ├─ scene.json
│              ├─ layers/
│              │  ├─ 1/
│              │  │  ├─ skeleton.skel
│              │  │  ├─ skeleton.atlas
│              │  │  └─ 原图集页.png
│              │  └─ 2/...
│              └─ backgrounds/
│                 └─ 1.png
├─ items/
│  ├─ items.json
│  └─ 302002.png
├─ gifts/
│  ├─ gifts.json
│  └─ 303013.png
├─ decorations/
│  ├─ decorations.json
│  ├─ hands/<ID>.png
│  ├─ avatar-frames/<ID>.png
│  ├─ riichi-sticks/<ID>.png
│  ├─ tables/<ID>.png
│  ├─ tile-backs/<ID>.png
│  ├─ tile-faces/<ID>.png
│  ├─ win-effects/<ID>.png
│  ├─ riichi-effects/<ID>.png
│  ├─ call-indicators/<ID>.png
│  ├─ lobby-backgrounds/<ID>.png
│  └─ music/<ID>.png
├─ illustrations/
│  ├─ illustrations.json
│  └─ <ID>.png
├─ titles/
│  ├─ titles.json
│  └─ <ID>.png
└─ ranks/
   ├─ ranks.json
   └─ <ID>.png
```

实际目录由本次选择及保留内容决定。仅动态导出可只有 spine，分类按实际输出写入 manifest；读取时以 JSON 引用为入口。

### 1.3 从哪个文件开始读取

```text
manifest.json
├─ characters[角色ID] → character.json
│                       ├─ skins[皮肤ID] → skin.json
│                       │                  ├─ images → PNG
│                       │                  └─ spine → scene.json → 骨架/atlas/纹理
│                       └─ 礼物/材料ID → 分类表 item_assets → PNG
└─ categories[分类] → 分类 JSON
                       ├─ entries → 业务条目与图片引用
                       ├─ assets → 图片尺寸与来源信息
                       ├─ item_assets → 按ID直接找图
                       └─ music → 音乐名称与图标引用
```

首先读取 manifest 并检查格式版本，然后按需要加载角色、分类、皮肤和场景。图片和骨架可按当前展示需求延迟加载。

## 2. 通用数据约定

### 2.1 JSON、路径与文件名

JSON 使用 UTF-8。业务顺序由排序字段及有序数组表达，例如 `scene.layers` 的顺序就是图层绘制顺序。

所有用于读取包内文件的路径都相对于 **素材包根目录**，统一以 `/` 分隔。例如 `skin.json` 中的 `characters/20000108/skins/40010803/full.png` 从包根解析。

`source_path`、`logical_path`、bindings 中的 `via` 和场景的 `order_source` 用于来源追溯与说明。实际文件定位使用 file、images、spine、skeleton、atlas、pages 和背景 path 等引用字段。

图片统一导出 PNG。单图直接使用 `<ID>.png`；皮肤按用途使用 `full.png`、`portrait.png`、`avatar.png`。同一用途确有多张时才追加 `-2`、`-3` 等编号。Spine 图集页保留 atlas 引用的页名。

未关联业务 ID 的条目可能使用原目录末段作为键，文件名中的不合法字符会被替换；无正式角色归属时使用 `characters/unassigned/`。消费端通过 JSON 引用和外键确定身份与文件位置。

### 2.2 ID 的实际类型

v2 当前没有把所有 ID 统一成同一种 JSON 类型：

| 位置 | 当前类型 |
| --- | --- |
| manifest.characters、character.skins、item_assets 的映射键 | string |
| character.id、礼物/材料中的 id、初始/契约皮肤 ID | integer |
| skin.id、skin.character_id、分类 entry.id | string，可含未关联时的目录键或 unassigned |
| binding.id、music 记录的 id | integer |
| entries、music 的映射键 | string，通常包含类别或配置表名前缀 |

消费端建立索引时可将业务 ID 规范化为字符串，带有类别或表名前缀的映射键则保留完整形式。

### 2.3 缺失、null 与顺序

- 字段表中的 `integer | null`、`boolean | null` 表示当前生产者可写出显式 null。null 表示配置值未知，消费端应保留这一状态。
- 可选字段可以不存在。例如仅动态皮肤没有 `images`，只有静态皮肤没有 `spine`。
- 空对象或空数组表示该集合没有记录。它可能来自筛选、没有素材或处理失败，需结合 status、errors 和 warnings 判断。
- manifest 列出素材包当前提供的分类；游戏版本的完整资源范围需要另行查询配置。
- 普通图片数组用于稳定列举。角色按 sort 排序，缺失 sort 可放末尾并以 ID 稳定排序。

### 2.4 关联依据 Binding

多个文件中的 `bindings` 共用以下结构：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| table | string | 来源配置表，如 item_definition.skin |
| id | integer | 该表中的记录 ID |
| name | string | 对应配置身份的显示名 |
| via | string | 建立关联所用的路径或关系说明 |

bindings 保留共享资源的各个配置身份及关联依据。消费端保存完整数组，并将 `via` 作为说明文字展示。

## 3. 根入口 manifest.json

### 3.1 字段定义

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| schema | string | 固定为 majsoul-gallery-assets |
| schema_version | integer | 当前为 2 |
| status | string | complete、partial 或 cancelled，见下节 |
| mode | string | 本次操作模式：incremental 或 full |
| unity | string | 来源安装的 Unity 版本 |
| categories | object<string, string> | 分类键 → 分类 JSON 的根相对路径 |
| characters | object<string, string> | 角色键 → character.json 的根相对路径 |
| filters | object | 本次导出的四个选项，见第 7.1 节 |
| errors | array<object> | 本次操作记录的处理错误 |
| limitations | array<string> | 素材包能力限制的说明文字 |

categories 的常用键为 `items`、`gifts`、`decorations`、`illustrations`、`titles`、`ranks`。角色入口使用独立的 characters 映射；静态和动态皮肤共用角色树，没有单独的根 spine 分类文件。

下面是只导出段位的一份完整根入口示例：

```json
{
  "schema": "majsoul-gallery-assets",
  "schema_version": 2,
  "status": "complete",
  "mode": "incremental",
  "unity": "2022.3.62f2c1",
  "categories": {
    "ranks": "ranks/ranks.json"
  },
  "characters": {},
  "filters": {
    "players_only": true,
    "exclude_expired": true,
    "unlocked_gifts_only": true,
    "overwrite": false
  },
  "errors": [],
  "limitations": [
    "装扮仅导出图标，音乐不含音频",
    "动态皮肤需要 Spine runtime，Unity shader/粒子未完整恢复"
  ]
}
```

### 3.2 状态与错误的含义

| status | 当前含义 |
| --- | --- |
| complete | 本次任务正常结束，未记录导出级错误 |
| partial | 本次任务结束时有导出级错误，或全量清理已开始但尚未完成 |
| cancelled | 本次任务处理了取消请求并写出了取消状态 |

complete 的判定范围是本次任务。导入时还需校验保留内容的引用，并读取动态场景的 warnings；部分表现缺失的可播放场景也可随任务正常完成。

errors 是异构对象数组，每项都有 error 文字，并按出错位置附带 resource、skin 或 item_id；也可能只有全局 error。消费端按各项实际提供的定位字段显示错误。

```json
[
  { "resource": "myassets/deco/example/pic/common/icon.jpg", "error": "找不到图片对象" },
  { "skin": "40010803", "error": "没有可播放图层" },
  { "item_id": 303013, "error": "物品图集未找到此图标" },
  { "error": "导出期间资源变化，请重建索引" }
]
```

进程被强制终止、元数据写入失败或任务提前返回错误时，manifest 可能保留上次状态；此类失败结合调用结果或终端日志诊断。导出一致性边界见第 7.4 节。

### 3.3 来源与选项的范围

filters、mode、errors 描述最近一次写入 manifest 的导出操作。需要记录游戏版本、区服、源目录、逐分类验收状态和文件校验和时，由消费端在导入流程中另行保存。

## 4. 角色与皮肤

### 4.1 character.json

manifest.characters 的值指向角色资料，通常位于 `characters/<角色ID>/character.json`。有完整角色配置时，字段如下：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| id | integer | 角色 ID |
| name | string | 角色显示名 |
| sort | integer / null | 配置排序值 |
| sex | integer / null | 原始性别编码，已验证安装中 1 女、2 男 |
| limited | boolean / null | 配置中的限定属性，null 表示未知 |
| collaboration | integer / null | 原始联动字段，大于 0 可显示联动标记 |
| treasure_sp | integer / null | 保留的原始配置值，获取方式的业务含义待核对 |
| favorite_type | integer | 喜好的礼物类型 |
| gifts | array<NamedItem> | 喜好礼物列表，受带锁礼物过滤影响 |
| can_contract | boolean | 角色配置是否提供契约相关能力 |
| contract_materials | array<Requirement> | 契约材料需求组 |
| raw_contract_materials | string | 原始材料表达式，供追溯 |
| contract_cost | integer | 原始费用数值，币种需结合客户端业务解释 |
| initial_skin | integer | 配置中的初始皮肤 ID |
| contract_skin | integer | 配置中的契约皮肤 ID |
| skins | object<string, string> | 皮肤 ID/键 → skin.json 的根相对路径 |

对于 unassigned 等没有完整正式角色资料的节点，文件可能只包含 skins 等少数字段。消费端保留节点及其可用资料，缺失属性展示为未知。

### 4.2 喜好、契约材料与按 ID 查图

NamedItem 在导出文件中只包含：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| id | integer | 物品 ID |
| name | string | 物品名称 |

图像通过下文的 item_assets 查询。Requirement 包含 `quantity: integer` 和 `items: array<NamedItem>`；quantity 是这一组的合计需求量。items 中的多个 ID 可混用，以合计数量满足 quantity。

下面是一个材料组的结构示例：

```json
{
  "quantity": 5,
  "items": [
    { "id": 303113, "name": "香喷喷曲奇" },
    { "id": 303013, "name": "香喷喷曲奇" }
  ]
}
```

查图流程为：

1. 将物品 ID 转为字符串，例如 `303113` → `"303113"`。
2. 通过 manifest.categories 找到 gifts 或 items 的分类 JSON；对未预先知道类型的材料，可依次检查两张表。
3. 查询该表的 `item_assets[ID]`，得到包根相对 PNG 路径。
4. 找不到时显示名称或占位图，并记录未解析引用。

开启仅普通礼物图标时，gifts 列表移除带锁礼物；契约材料里的替代 ID 仍保留，并可映射到普通图像。例如两个 ID 都可能指向 `gifts/303013.png`。关闭过滤时，带锁礼物及其原始锁图会独立导出。

角色依赖的物品图标即使没有选择独立的道具/礼物分类，也会自动补齐并生成相应分类表。因此 categories 中可能出现用户未显式勾选的 items 或 gifts。

### 4.3 skin.json

每套皮肤位于 `characters/<角色键>/skins/<皮肤键>/`，静态与动态表现共用一个 skin.json。

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| id | string | 皮肤 ID；未关联时可能是目录键 |
| name | string | 当前条目的显示名 |
| character_id | string | 正式角色 ID，或 unassigned |
| bindings | array<Binding> | 配置关联依据 |
| images | object<string, array<string>>，可选 | 静态图片用途 → 根相对路径数组 |
| spine | string，可选 | scene.json 的根相对路径 |

images 中当前输出 `full`、`portrait`、`avatar`：分别对应完整立绘、大头像、小头像。即使只有一张，也使用数组。某种用途没有成功输出时，该键可以不存在；整个 images 也可能为空。

```json
{
  "id": "40010803",
  "name": "元宵 · 避暑大作战！",
  "character_id": "20000108",
  "bindings": [],
  "images": {
    "full": ["characters/20000108/skins/40010803/full.png"],
    "portrait": ["characters/20000108/skins/40010803/portrait.png"],
    "avatar": ["characters/20000108/skins/40010803/avatar.png"]
  },
  "spine": "characters/20000108/skins/40010803/spine/scene.json"
}
```

这是引用结构示例，bindings 内容为简洁起见未展开。只导出动态皮肤的新目录可没有 images，只导出静态时可没有 spine；先后分别导出时，两者合并到同一文件。

静态输出范围为 images 中的 full、portrait、avatar，消费端按用途选择主图或头像。

## 5. 道具、礼物及其他分类

### 5.1 分类表的共同结构

`items.json`、`gifts.json`、`decorations.json`、`illustrations.json`、`titles.json`、`ranks.json` 共用以下顶层对象：

| 字段 | 类型 | 用途 |
| --- | --- | --- |
| entries | object<string, Entry> | 业务条目目录：有哪些内容、叫什么、引用哪些图片 |
| assets | object<string, Asset> | 图片资料：路径、尺寸及可选来源信息 |
| item_assets | object<string, string> | ID 到主图标路径的直接查找表 |
| music | object<string, Music> | 音乐身份、名称和条目/图标的关联 |
| definitions | object<string, ItemDefinition> | 道具与礼物的完整业务定义，以物品 ID 为键；其他分类为空 |

没有内容的对象通常为空对象。item_assets 提供当前分类的 ID 查图接口，例如 ranks.json 使用段位记录 ID。消费端按分类建立查找表，保留 ID 的分类上下文。

`ItemDefinition` 包含 `id`、`name`、`sort`、`group`、`gift_type`、`gift_bond`、`rarity`、`locked`、`description` 和 `expires`。`group` 为 `materials`、`gifts`、`vouchers`、`containers`、`activity` 或 `other`；礼物的 `rarity` 为 `green`、`blue`、`purple` 或 `special`，其他道具为 null。`gift_bond` 是基础赠礼好感值，`locked` 表示锁定礼物。`description` 和 `expires` 保存配置中的说明与到期时间，未配置时为空字符串。

消费端从 definitions 建立道具目录，按同表的 `item_assets[ID]` 查图。缺少本地图标的道具仍可包含定义。普通与锁定礼物保留独立 ID，玩家库存分别关联；契约计算使用角色资料中的等价 ID 组。主应用导入时包含锁定礼物，角色喜好区仅展示普通图标。

entries 的键通常为 `<内部类别>:<业务ID或资源键>`。内部类别与根分类键不同，例如 `tile-back` 属于 decorations，`loading` 属于 illustrations。该键作为整体标识保存，来源表通过 bindings 查询。

### 5.2 Entry 与 Asset 字段

| Entry 字段 | 类型 | 当前含义 |
| --- | --- | --- |
| id | string | 条目主 ID 或未关联时的资源键 |
| name | string | 显示名 |
| assets | array<string> | 该条目的图片路径，需在根目录解析 |
| category | string，可选 | 正常分类条目的内部类别 |
| bindings | array<Binding>，可选 | 正常分类条目的来源关联 |

通过角色依赖补齐的简化 Entry 可能只有 id、name、assets。多个配置身份共享一套图片时，通过 bindings 和 music 保留完整身份关系。

| Asset 字段 | 类型 | 当前含义 |
| --- | --- | --- |
| file | string | 包根相对路径，通常与 assets 映射键相同 |
| width、height | integer | 导出图片的像素尺寸 |
| kind | string | full、portrait、avatar、icon、image 等用途；依赖图标通常为 icon |
| role | string，可选 | 面向人的用途说明文字 |
| locale | string，可选 | chs 或 common 等来源变体标记 |
| source_path | string，可选 | 原始逻辑路径，可能指虚拟图标，供来源追溯 |

角色依赖补齐的简化 Asset 通常只有 file、width、height、kind。其余字段按可选字段处理。locale 描述来源变体，图片中的实际文字可通过预览确认。

以下为使用真实段位字段组织的缩减示例：

```json
{
  "entries": {
    "rank:10101": {
      "id": "10101",
      "name": "初心",
      "category": "rank",
      "bindings": [
        {
          "table": "level_definition.level_definition",
          "id": 10101,
          "name": "初心",
          "via": "primary_icon 去重 → 大段位徽章（不区分星级和魂天等级）"
        }
      ],
      "assets": ["ranks/10101.png"]
    }
  },
  "assets": {
    "ranks/10101.png": {
      "file": "ranks/10101.png",
      "width": 256,
      "height": 252,
      "kind": "icon"
    }
  },
  "item_assets": {
    "10101": "ranks/10101.png"
  },
  "music": {}
}
```

示例中的图片元数据只展示共同字段。宽高仅用于说明单位，消费端读取实际文件中的值。

### 5.3 装扮、插画、称号和段位的区别

| 根分类/子目录 | 内部 category | 包内内容 |
| --- | --- | --- |
| decorations/hands | hand | 手样式展示图标 |
| decorations/avatar-frames | frame | 图标及可用框图 |
| decorations/riichi-sticks | stick | 立直棒图标 |
| decorations/tables | table | 桌布图标 |
| decorations/tile-backs | tile-back | 牌背图标 |
| decorations/tile-faces | tile-face | 牌面图标 |
| decorations/win-effects | win-effect | 和牌特效图标 |
| decorations/riichi-effects | riichi-effect | 立直特效图标 |
| decorations/call-indicators | call-effect | 鸣牌指示图标 |
| decorations/lobby-backgrounds | background / background-icon | 大厅背景展示图标 |
| decorations/music | bgm / riichi-bgm | 音乐图标，曲目关系另见 music |
| illustrations | loading | 插画主图及可导出的缩略图 |
| titles | title | 称号相关图片，可能包含不同用途 |
| ranks | rank | 按 primary_icon 去重的大段位徽章，保留四麻和三麻差异 |

插画 Entry 另含 `loading_image_id`（配置中的插画 ID）、`sort`（展示顺序）、`unlock_items`（可解锁这张插画的物品 ID 数组）及 `images`。`images.full` 和 `images.thumbnail` 是包根相对图片路径，分别按客户端配置的 `img_path`、`thumb_path` 匹配。卡片使用 thumbnail，预览和导出使用 full；缺少缩略图时可使用原图。同一目录可能混有表情等附加图片，消费端通过明确的 images 映射选图。

`id` 保留条目的业务 ID。消费端使用 `sort` 排序，并将 `unlock_items` 与账号背包关联；账号的 `loading_image` 列表对应 `loading_image_id`，表示当前选用的插画。

多图条目通过 Entry.assets 和 Asset.kind/role 区分用途。文件名中的 `-2` 表示同一条目下的附加图片。

### 5.4 音乐名称与图标

decorations.json 的 music 保存音乐关联，键为 `<配置表名>:<ID>`：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| id | integer | 来源表的记录 ID |
| name | string | 对应名称 |
| table | string | 来源表，如 audio.bgm 或 item_definition.item |
| entry | string | 指向同一分类表中的 entries 键 |
| assets | array<string> | 关联图标路径 |

音乐条目从其 bindings 生成关联，可能保留多个来源身份。一个图标可对应多首音乐，曲目按 music 记录统计。当前素材包提供音乐名称与图标，音频播放数据需另行取得。

## 6. 动态皮肤场景

### 6.1 scene.json 的入口与结构

通过 skin.spine 找到 scene.json。静态图片和场景可以独立存在，消费端可在不支持 Spine 时继续展示该皮肤的静态图。

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| layers | array<Layer> | 按绘制顺序排列的骨架图层 |
| backgrounds | array<Background> | 已恢复的静态背景，允许为空 |
| warnings | array<string> | 缺层、背景读取失败或未恢复表现的提示 |
| order_source | string | 图层顺序的配置来源说明 |

下面是单层、无背景场景的结构示例。page.png 为示例页名，实际以 atlas 中的引用为准。

```json
{
  "layers": [
    {
      "skeleton": "characters/20000108/skins/40010803/spine/layers/1/skeleton.skel",
      "atlas": "characters/20000108/skins/40010803/spine/layers/1/skeleton.atlas",
      "pages": {
        "page.png": "characters/20000108/skins/40010803/spine/layers/1/page.png"
      },
      "logical_path": "myassets/spine/example/example.skel.txt",
      "version": "4.2.08",
      "format": "skel",
      "premultiplied_alpha": false,
      "cached": false
    }
  ],
  "backgrounds": [],
  "warnings": [],
  "order_source": "客户端编号 character.prefab 顺序"
}
```

### 6.2 Layer 字段与播放要求

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| skeleton | string | 骨架文件的包根相对路径 |
| atlas | string | atlas 文件的包根相对路径 |
| pages | object<string, string> | atlas 原始纹理页名 → 导出纹理根相对路径 |
| logical_path | string | 原始骨架逻辑路径，仅供追溯 |
| version | string | 实际骨架版本，如 4.2.08 |
| format | string | 当前为 skel 或 json，决定骨架读取器 |
| premultiplied_alpha | boolean | 当前图层所需透明度混合模式 |
| cached | boolean | 生成该场景时的解析缓存状态，仅诊断用 |

当前导出器支持 Spine 4.2 系列。消费端应选择兼容 runtime，并根据 format 读取二进制或 JSON 骨架。cached 供生成端诊断使用；Gallery 的文件读取与缓存状态由消费端自行管理。

pages 的键与 atlas 页名一致，消费端可将其映射到根相对路径对应的 URL，或利用同层文件布局加载。加载时保留 atlas 的原始页名、区域、旋转和声明尺寸。

layers 数组确定绘制顺序，目录 1、2 按该顺序编号。多个骨架应共享坐标系、缩放和相机；逐层各自适应窗口后叠加会破坏合成位置。消费端从骨架读取动作列表，选择主层动作并同步其他层的同名动作，缺少时退回 idle 或初始姿态。

### 6.3 Background 字段与坐标

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| path | string | 背景 PNG 的包根相对路径 |
| logical_path | string | 原始纹理逻辑路径，仅供追溯 |
| x、y | number | 场景坐标中的矩形左下角 |
| width、height | number | 场景坐标中的矩形尺寸 |
| uv | array<number>，4 项 | scaleX、scaleY、offsetX、offsetY |
| color | array<number>，4 项 | r、g、b、a 材质颜色，alpha 已包含提取的透明度系数 |
| order | integer | 源材质 render queue；背景数组按它升序排列 |

背景和骨架使用共同场景坐标，显示时统一转换到视口。当前预览先按 backgrounds 数组绘制背景，再按 layers 绘制骨架。order 用于排列背景。

当前 WebGL 参考实现对导出 PNG 使用以下 UV 端点：

```text
u0 = offsetX
v0 = 1 - offsetY
u1 = offsetX + scaleX
v1 = 1 - offsetY - scaleY
```

换用不同纹理坐标方向的渲染器时，应相应转换，避免再次错误翻转。backgrounds 列表保存当前成功恢复的轴对齐 Quad 背景；缺失记录可结合 warnings 和原始场景进一步核对。

### 6.4 图层警告与表现范围

场景允许部分图层成功、其他图层失败，缺层详情记录在 warnings。背景恢复失败、isEffect、isMask 等限制也会产生 warnings；存在可用图层时，导出可继续完成。

消费端在播放时同时呈现 warnings，说明缺失图层或效果。Unity shader 动画、粒子、场景遮罩、任意网格和其他交互状态属于后续恢复范围。

## 7. 导出选项与目录更新语义

### 7.1 分类选择与默认过滤

GUI/CLI 分类选择名与根目录入口的对应关系如下：

| 选择名 | 结果入口 |
| --- | --- |
| character | characters → 静态皮肤 |
| spine | 同一个 characters → 动态皮肤 |
| item / gift | categories.items / categories.gifts |
| decoration | categories.decorations |
| illustration | categories.illustrations |
| title / rank | categories.titles / categories.ranks |

默认八类全选。manifest.filters 记录如下四个选项：

| 字段 | 类型 | 默认值 | 含义 |
| --- | --- | --- | --- |
| players_only | boolean | true | 只导出已关联正式角色资料、性别为已知玩家角色编码且非剧情条目的角色/动态皮肤 |
| exclude_expired | boolean | true | 过滤可识别为已过期的道具；未知日期格式保留 |
| unlocked_gifts_only | boolean | true | 礼物仅保留普通图标，角色 gifts 同步过滤；契约替代 ID 保留 |
| overwrite | boolean | false | false 为增量，true 为全量清理重建 |

unlocked_gifts_only 筛选图标的带锁变体，到期判断按客户端 UTC+8。玩家实际解锁状态通过账号资料关联。

固定选图规则为：优先简体/通用图；角色仅导出 full/bighead/smallhead 对应图片；装扮保留可用图标及头像框框图，排除 3d/texture 等表现贴图。手样式仅保留有相应展示图标的条目。

### 7.2 增量导出

增量是默认行为：

- 对当前计划中的图片，通过读取目标尺寸判断是否复用。需要确认与更新后的源图一致时，选择全量重新生成。
- 已有 Spine scene.json 可解析、包含非空图层，并且其中骨架、atlas、页面及背景文件均存在时，复用整个场景。该检查验证场景记录及文件存在性；重新生成使用全量模式。
- 当前处理的角色资料、分类表和 manifest 会更新；未选分类入口及文件通常保留，既有皮肤的另一种表现也可保留。
- 增量保留现有文件。更严格的过滤会更新 JSON 的引用范围，原来输出的部分图片可能因此成为磁盘上的未引用文件。

当前处理的分类表由本次条目和必要依赖重新生成，旧表中的额外条目可能随之移除。

手工修改过的现有图片也按可读性判断复用。需要恢复源图时，对相应分类执行全量。

### 7.3 全量导出

全量先清理所选范围，再重新生成；未选择的其他分类保留。

| 选择范围 | 清理结果 |
| --- | --- |
| item、gift、decoration、illustration、title、rank | 分别清理对应分类目录，并移除其旧 manifest 分类入口 |
| character 和 spine 同时选择 | 清理整个 characters 树 |
| 只选 character | 清理静态图片及旧辅助文件，保留动态 spine 子树，并重建对应皮肤资料 |
| 只选 spine | 清理动态 spine 子树和 skin.spine，保留静态图片及其资料 |

清理某一种皮肤表现后，若皮肤不再有另一种可保留内容，则移除该空皮肤节点；角色没有保留皮肤时也可被移除。旧 image.png 等非现行静态文件会随静态清理删除。

角色礼物/契约图标属于依赖，即使未单独选择 item/gift，仍可能补写这些目录和分类表。全量清理了 item/gift 后，仍保留的角色资料所需图标也会尝试补齐。依赖补齐可能更新未显式选择的 items/gifts 分类表。

清理限制在导出根目录内，遇到符号链接或 Windows 联接点时拒绝清理。游戏安装保持只读，查看器预览缓存继续保留，已转换的素材可复用后写出。

### 7.4 写入、取消和异常中断

正常执行先处理素材，随后写出角色/分类 JSON，最后更新 manifest；动态场景 JSON 会在对应场景完成时写出。JSON 使用临时文件原子替换，PNG 分别复制；整个目录在任务期间逐步更新。

全量清理开始前会先将 manifest 标记为 partial。正常处理取消时保留已完成文件并写出 cancelled；可继续处理的单项失败记录到 errors，最终标记 partial。

增量过程中若进程崩溃或被直接结束，旧 manifest 可能仍显示 complete，而目录已经部分更新；其他提前失败也可能来不及写出新错误。消费端应在导出任务结束后校验引用，再导入素材包。

### 7.5 dry-run 的作用

CLI 的 export --dry-run 共用分类过滤、命名、图集定位和增量检查，显示预计图片、可复用文件、动态场景及额外图标依赖。输出目录在预演期间保持原状，全量选项仅用于计算计划。

预演可更新查看器自身的配置缓存。纹理解码、动态皮肤完整依赖和最终渲染通过实际导出验证；命令用法见工具 README。

## 8. 消费端校验

### 8.1 建议的导入步骤

1. 找到 manifest.json，检查 schema 为 majsoul-gallery-assets、schema_version 为 2，确认导出任务已经结束。
2. 读取 status、errors、limitations。对 partial/cancelled 明确提示，并由用户选择是否接受部分内容。
3. 按 manifest 提供的角色和分类入口建立业务目录。
4. 规范化 ID，建立角色、皮肤、条目和 item_assets 索引；容忍简化角色/依赖记录中的可选字段缺失。
5. 验证静态图片引用、材料图标查找和场景依赖。将读取失败的引用记录为缺失。
6. 对 Spine 检查版本及 format、页名映射和图层顺序；保留 warnings，让不支持动画的环境仍可使用静态素材。
7. 在消费端完成校验后，再将正式导出的素材包纳入自身持久资源库。

### 8.2 路径与引用检查

所有实际文件引用应是包内相对路径。拒绝绝对路径、盘符/UNC 路径、URL、`.`/`..` 越界片段；若消费端允许文件链接，还需要检查最终解析目标仍在包根目录内。来源说明字段按第 2.1 节作为追溯数据展示。

同时核对：Entry.assets 是否指向存在图片，Asset.file 是否与其索引键一致，character.skins 是否能读取 skin.json，skin.character_id 是否与父角色键一致，scene 的骨架、atlas 和全部 pages 是否齐全。带锁 ID 到普通图标的多对一映射按合法引用处理。

### 8.3 当前格式要求

消费端只读取本文定义的 v2 素材包。其他版本应重新导出。

必要字段应做类型检查；可选字段按本文规定的缺省行为处理。语义未知的游戏配置值原样保留。

本文中的示例用于说明结构；实际入口、可选字段和素材引用以每份文件为准。文档核对依据为 [导出实现](../crates/majsoul-assets/src/core/export.rs)、[角色资料结构](../crates/majsoul-assets/src/core/characters.rs)、[Spine 场景结构](../crates/majsoul-assets/src/core/spine.rs) 与 [背景结构](../crates/majsoul-assets/src/core/spine_background.rs)。
