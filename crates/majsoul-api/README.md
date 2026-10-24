# majsoul-api

独立的国际服雀魂账号只读客户端。提供当前 Unity 线路发现、账号密码认证、心跳和完整账号查询；结果由使用方转换为自己的业务模型。

```rust
use majsoul_api::{Client, Policy, Snapshot, Zeroizing, discover};

async fn synchronise(username: &str, password: String) -> anyhow::Result<Snapshot> {
    let policy = Policy::default();
    let route = discover(&policy).await?;
    let mut client = Client::connect(route, policy).await?;
    // 使用方可在此设置持久化的设备标识：client.set_device_id(...)?。
    client.login(username, Zeroizing::new(password)).await?;
    let snapshot = client.sync().await?;
    client.close().await;
    Ok(snapshot)
}
```

长连接的使用方按 `heartbeat_interval()` 调用 `heartbeat()`。认证和同步错误由调用方展示；超时或断线后重新建立连接。默认请求间隔 1.2 秒，完整同步间隔 60 秒，进程内登录冷却 30 秒。

```powershell
cargo test -p majsoul-api
cargo run -p majsoul-api --example probe
```

`probe` 只检查公开配置、路由握手和心跳。当前已验证此路径；真实账号认证与完整查询待账号验收。

实现和协议来源见[账号 API 文档](../../docs/account-api.md)，协议提取来源及许可见 [protocol/NOTICE.md](protocol/NOTICE.md)。
