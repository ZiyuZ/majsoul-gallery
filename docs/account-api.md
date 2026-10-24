# 国际服账号 API

更新日期：2026-10-24。

## 目标与当前状态

账号接入服务于收藏与角色养成：读取账号资料、角色与皮肤、星标、好感、契约状态、装扮和材料库存，生成可离线使用的账号快照。素材由应用维护的唯一素材库提供，网络补充素材单独推进。

当前已实现国际服账号密码认证、只读查询、GUI 登录与同步、按账号保存，以及基础请求限制。已通过真实国际服的线路发现、WebSocket 握手、路由连接和心跳检查；认证成功后的完整链路仍需真实账号验收。日常单元测试使用本地模拟服务器。

## 模块边界

`crates/majsoul-api` 是独立 Rust crate，负责以下内容：

- `discovery.rs`：读取客户端版本、配置和可用线路。
- `metadata.rs`：在内存中读取 Unity 资源清单及版本／配置包，复用已有 Unity 读取库。
- `protocol.rs`：依据内置描述符编码请求、解析响应，关联消息序号。
- `client.rs`：线路握手、认证、串行查询与完整 API 快照。
- `policy.rs`：请求间隔、同步间隔和登录冷却。
- `protocol/liqi.proto`：从当前 Unity 客户端提取的协议定义。

主应用的 `src/account/network.rs` 维护一个后台连接，接收同步与取消指令，并发送进度、结果和错误。`src/account/api.rs` 将协议结果转换为界面和报表使用的账号快照。API crate 可通过单独的 Cargo 命令构建与测试。

## 连接与认证

首次打开登录／同步界面时后台预取公开版本与线路。成功结果在本进程缓存，登录请求等待或复用同一份结果；路由连接或心跳失败后清空缓存，下一次连接重新获取。

当前协议基准为国际服 Unity `4.0.51`。运行时读取版本，避免将登录使用的资源版本写死在程序中。

1. 从 `https://game.maj-soul.com/1/` 读取 Unity 包版本与发布标识。
2. 经 `clientBundleSettings` 和 `warehouseSettings` 找到资源仓库，读取 `bundle_info_so.majset`。
3. 从资源清单定位 `MyAssets/docs/config.json` 与 `MyAssets/docs_version/version.json` 的所属包，读取真实网关配置和资源版本。
4. 从 `gateways` 中选择 HTTPS 入口，访问 `/api/clientgate/routes` 获取线路。
5. 按服务端顺序选择可用的 TLS 线路，连接 `wss://<domain>/gateway`，使用 `ws` 子协议。
6. 调用 `Route.requestConnection`，提交线路 ID、平台和时间戳，确认连接可用。
7. 调用 `Lobby.login`，提交账号和 HMAC-SHA256 派生的密码、设备信息及客户端版本。
8. 完成 `loginBeat` 与 `loginSuccess` 生命周期，然后查询账号数据。

以上流程由当前客户端代码与公开项目交叉核对。旧示例中的 `region_urls` 和旧版 Web 客户端版本字符串需要按这一流程适配。2026-10-24 实测，旧网页 `version.json` 仍为 `0.11.252.w`，Unity 包内版本已为 `0.16.289`，因此认证使用 Unity 包内的版本。

这一步读取资源清单与两个版本／配置包，全部在内存中解析并释放，复用现有 Unity 读取依赖。

登录按输入格式区分账号／邮箱（`type=0`）与国内手机号（`type=1`），与注册接口的 `1/2` 编号分别处理。设备标识在应用设置中生成并持久保存，供后续会话复用。密码在提交后清空输入框，认证结果中的令牌和密码派生值不写入账号快照。可选记住密码，成功同步后将 HMAC 派生值明文存入独立的 `accounts/credentials.json`，按区服与登录名区分；取消勾选删除当前账号记录。该派生值可以直接认证，敏感程度等同密码。默认重新启动离线；开启自动登录后，素材载入完成会使用已记住的派生值尝试登录一次，失败后显示登录界面并等待手动处理。登录框以掩码表示已有记录，修改密码会使用新输入。

登录成功后，后台保留同一条连接，每 15 秒发送心跳，手动同步与定时同步均复用该会话。客户端本地错误表 `info.error` 对应中文表 `info_error_chs`：`1002` 表示账号不存在（包括邮箱后缀大小写提示），`1003` 表示密码错误。2026-10-24 修正了将注册账号类型用于登录的错误；该错误可能使服务端在错误的账号类型中查找账号并返回 `1002`。

## 查询结果如何映射到养成数据

每轮依次调用 `fetchAccountInfo`、`fetchCharacterInfo`、`fetchBagInfo` 和 `fetchTitleList`。全部成功且 UID 一致后返回完整快照。

| 数据 | 来源与转换 |
| --- | --- |
| 昵称与 UID | `Account.nickname`、`account_id` |
| 四麻与三麻段位 | `Account.level`、`level3`；按大段位关联徽章，保留小段位与 `score` |
| 已拥有角色、契约 | `characters[].charid`、`is_upgraded` |
| 星标与用户顺序 | `ResCharacterInfo.character_sort` 为星标列表，`other_character_sort` 为其他角色列表；依次保留两段原始顺序，新角色回退到配置 `sort` |
| 累计好感 | `characters[].level/exp`；在当前爱心等级的累计起点上加等级内经验，上限 88,000 |
| 已拥有皮肤 | `skins`、角色当前 `skin`，以及本地角色配置中的初始皮肤和已契约角色的契约皮肤 |
| 材料库存 | `bag.items[].item_id/stack`；相同物品 ID 合并数量 |
| 装扮拥有情况 | 以库存物品 ID 关联装扮配置，并补充当前头像框 |
| 同步时间 | 完成整轮查询时的本地时间 |

好感等级的累计起点为 0、2,000、8,000、18,000、38,000、88,000。原始协议结果也保留在账号缓存中，便于后续验证特殊角色与字段变化。装扮中的默认拥有项、音乐及实际账号中的特殊库存表现，需要在真实账号验收时继续校对。

账号缓存组织为：

```text
majsoul-gallery-data/
├─ settings.json                    账号名、设备标识及界面设置
└─ accounts/
   ├─ selected.json                 最近成功同步的区服与 UID
   └─ cn/
      └─ <UID>/
         └─ account.json            界面快照、原始查询结果、同步周期
```

一次同步失败会保留已有快照。登出会关闭连接并恢复离线界面；缓存继续保留。命令行的已拥有清单默认读取最近成功同步的账号，也支持通过 `--account` 指定缓存或独立快照文件。

## 请求限制与失败处理

当前默认策略如下：

| 操作 | 默认限制 |
| --- | --- |
| HTTP 与 RPC 请求 | 串行；相邻请求至少间隔 1.2 秒 |
| 登录尝试 | 进程内共享 30 秒冷却；认证失败后等待手动重试 |
| 完整账号同步 | 同一连接至少间隔 60 秒；失败的同步也计入间隔 |
| 自动同步 | 关闭、15 分钟、1 小时、3 小时；由用户选择 |
| 路由心跳 | 15 秒，参照当前客户端的正常线路心跳 |
| 网络超时 | 单次 20 秒 |
| 公开配置获取 | 短暂连接错误或 5xx 最多重试一次，等待 2 秒；最多尝试两个配置入口 |
| 429、维护及认证错误 | 显示错误并停止当前轮次；429 保留服务端等待时长 |
| RPC 通信错误或超时 | 结束当前连接，等待手动重新登录 |

收到其他客户端登录、账号登出或服务端关闭连接的通知时，会结束当前会话并保留快照，等待用户手动重新登录。

请求接口限定为认证生命周期、心跳和上述账号读取。当前接入保持单个活动连接，切换账号或登出会取消旧任务；过期结果无法覆盖当前账号的界面状态。

这些限制用于控制请求量。非官方客户端的账号风险仍由服务端规则决定，限流本身无法提供免封号保证。

## 验证与协议更新

离线测试：

```powershell
cargo test -p majsoul-api
```

无需账号的真实连接检查：

```powershell
cargo run -p majsoul-api --example probe
```

该检查执行线路发现、路由握手和一次心跳后关闭连接。真实账号测试从主应用左下角的登录入口进行；首次重点核对角色数、初始／契约皮肤、星标、好感、许愿石、粉尘和装扮拥有状态。

协议在开发阶段生成并随程序发布，构建时由 `protox` 编译成描述符。维护时先将 Akagi 克隆到 `.local/reference-repos/Akagi`，检查其 `scripts/extract_liqi.py`，再执行：

```powershell
uv run --with requests --with UnityPy==1.25.2 python .local/reference-repos/Akagi/scripts/extract_liqi.py
Copy-Item .local/reference-repos/Akagi/src/bridge/majsoul/proto/liqi.proto crates/majsoul-api/protocol/liqi.proto
```

更新后记录客户端版本和上游提交，更新 crate 中的协议版本常量与来源说明，运行离线测试及 `probe`。RPC 方法与类型由 `.proto` 的 service 描述符解析。

## 参考项目与致谢

感谢以下项目公开协议研究与实现。本项目的通信和账号映射逻辑使用 Rust 编写；当前协议定义由 Akagi 提取脚本重新生成，随文件保留其来源及许可说明。

| 项目 | 参考内容 | 仓库声明的许可 |
| --- | --- | --- |
| [Akagi，v3 分支](https://github.com/shinkuan/Akagi/tree/v3) | 当前 Rust 协议解析、从 Unity 提取描述符与服务表的脚本、协议更新组织 | Apache-2.0 |
| [mahjong_soul_api](https://github.com/MahjongRepository/mahjong_soul_api) | 国际服账号密码认证、HMAC 参数与基础 RPC 流程 | 未找到仓库级 LICENSE |
| [vg-mjg/majsoul-api](https://github.com/vg-mjg/majsoul-api) | 独立客户端边界、连接与编解码分层、心跳和错误事件 | MIT |
| [MajsoulMax](https://github.com/Avenshy/MajsoulMax) | 协议路由、请求序号配对、角色与装扮消息结构 | GPL-3.0 |
| [Naki](https://github.com/Sunalamye/Naki) | [Unity 协议研究](https://github.com/Sunalamye/Naki/blob/main/docs/majsoul-unity-protocol.md)、消息封装边界、协议漂移与测试组织 | AGPL-3.0 + Commons Clause，按仓库声明 |

Akagi 的客户端提取脚本及本地 Unity 代码也提供了版本适配依据：`GatewayFetcher`、`NetRoute`、`LoginMgr`、`GameUtility`、`GameMgr` 与角色排序界面用于核对当前行为。克隆的参考项目保留在研究目录中。

公开实现中的限制主要分布在心跳、连接重试或特定赛事查询中。`vg-mjg/majsoul-api` 的赛事跟踪存在 5 秒节流，OAuth 检查存在短暂等待；本应用另外定义完整同步与登录的请求策略。

账号目录中的 `cn` 标识当前接入的中文线路。界面统一称为账号登录，本文使用国际服描述对应线路。
