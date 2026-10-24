# Liqi 协议定义来源

`liqi.proto` 于 2026-10-24 使用 [Akagi 的提取脚本](https://github.com/shinkuan/Akagi/blob/v3/scripts/extract_liqi.py) 从国际服 Unity WebGL 客户端重新生成。

- 客户端入口：`https://game.maj-soul.com/1/`
- Unity 包版本：`4.0.51`
- 提取结果：11 个协议模块、3 个服务、443 个 RPC 路由。
- 上游提交：`6fef1c3f2f2e39d983dfbc1aee7d2468025193bf`。
- 上游版权声明：Copyright 2026 Shinkuan。
- 上游声明许可：Apache License 2.0，完整文本见 [LICENSE-Akagi.txt](LICENSE-Akagi.txt)。

生成时会过滤客户端裁剪掉类型的调试 RPC。客户端连接、请求限制、业务查询与快照映射由本项目独立实现。
