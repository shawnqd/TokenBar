# Browser Cookie Extraction（Windows）

本文是 V5 的认证与 Cookie 行为合同。默认来源与上游 Win-CodexBar 对齐
（2026-09-05 用户决策）：服务商默认只读取用户主动提供并保存的会话 Cookie；
自动读取浏览器会话是每个服务商的显式 opt-in，不默认开启。

`DEFAULT_COOKIE_SOURCE = "manual"`（`rust/src/settings.rs`）；上游保持同一默认
的原因：解密 Chromium Cookie 需要走 Windows DPAPI，可能触发杀毒软件告警，
浏览器读取必须由用户明确开启。

## 产品规则

- **手动是默认来源**：对支持会话 Cookie 的服务商，刷新默认读取用户粘贴或导入并
  保存的会话 Cookie；没有保存时回落到该服务商自己的登录阶梯（CLI/OAuth 等），
  不伪造成功值。
- **自动读取是显式 opt-in**：用户在该服务商的「Cookie 来源」里选择「自动读取浏览器」后，
  刷新时才自动寻找可用浏览器 Cookie 并在内存中组装请求头；选择状态持久化。
- **停用是来源状态，不是登录方式**：如果 provider 的运行时能力提供 `disabled`，
  设置页可让用户停用 Cookie 来源；停用后不得读取浏览器或已保存 Cookie，也不得把
  provider 伪装成“未登录成功”以外的状态。没有该选项时不画停用控件。
- **API 密钥是另一类凭据**：只有服务商真实支持 API 时才显示，不与 Cookie 读取
  位置混成一个登录分段。
- **CLI/OAuth 是服务商专用登录动作**：只有服务商确实支持时才显示。Codex 的
  CLI/OAuth 登录不能被网页 Cookie 登录替代；Codex 的 Cookie 只用于网页数据补充。
- **通用 WebView 登录不作为产品路径**：设置页、托盘和详情不显示通用「打开网页登录/
  捕获」入口，也不创建应用自有登录浏览器窗口。后加的批量导入与会话粘贴写同一受保护存储；
  自动模式只读取用户已经登录的浏览器配置，不负责替用户打开或操作浏览器。

## 界面术语与 `usage_source` 的边界

Cookie 读取位置和用量刷新通道是两个不同的设置，不能共用一个“自动”标签，也不能都放进
“登录方式”分段：

| 设置 | 用户可见标题 | 允许的文案 | 实际含义 |
| --- | --- | --- | --- |
| Cookie source | **Cookie 来源** | 自动读取浏览器 / 使用已保存 Cookie / 停用 Cookie 来源 | 决定从哪里取得会话 Cookie |
| Usage source | **用量读取方式** | 自动选择 / 网页数据 / 本机 CLI / OAuth 接口 | 决定刷新用量时使用哪条 provider 通道，不改变认证方式 |

这里的“自动选择”只对应 `usage_source=auto`；“自动读取浏览器”只对应
`cookie_source=auto`。后端没有返回某个能力时，页面不得自行补画该选项。

认证方式另行表达 API 密钥、会话 Cookie 或 provider 专用 CLI/OAuth/device 登录动作。布局上这些
区块可以继续出现在同一张 provider 详情卡中，但标题、说明和状态必须能让用户看出它们不是同一个
登录入口。

## 自动读取范围（用户选择「自动读取浏览器」后）

| 浏览器 | 读取方式 | 产品承诺 |
| --- | --- | --- |
| Chrome | DPAPI + AES-256-GCM | 自动尝试 |
| Edge | DPAPI + AES-256-GCM | 自动尝试 |
| Brave | DPAPI + AES-256-GCM | 自动尝试 |
| Firefox | `cookies.sqlite` | 自动尝试，并作为 Chromium 解密失败时的推荐回退 |
| Arc / Chromium | Chromium 配置文件方式 | 配置可读时自动尝试；不单独增加登录入口 |

自动探测只针对已启用的服务商和其允许的域名。浏览器顺序、Cookie 域名和必需
Cookie 名称由 provider 能力定义，不能由页面按服务商名称硬编码。

## 自动流程

1. 根据 provider 能力确定一个或多个 Cookie 域名。
2. 探测当前用户的浏览器配置目录和可用 profile。
3. Chromium 读取 `Local State`，使用当前用户 DPAPI 解密数据库密钥，再读取
   `Network\\Cookies`；Firefox 读取 profile 下的 `cookies.sqlite`。
4. 只保留目标域名的 Cookie，在内存中生成 `Cookie: name=value` 请求头。
5. 将请求交给 provider 的网页策略；成功结果更新缓存和统一快照。

自动读取只在用户为该服务商选择「自动读取浏览器」后发生；默认（使用已保存 Cookie）刷新不触发浏览器探测。
自动模式不会启动 Chrome、Edge 或其他独立浏览器，也不会把系统默认浏览器的登录页当成
应用内登录流程。

## App-Bound Encryption 与失败回退

Chrome/Edge 新版可能使用 App-Bound Encryption（例如 v20 Cookie），或者浏览器
正在锁定数据库。自动流程必须：

1. 记录本次浏览器和失败原因，但不把失败伪装成成功额度；
2. 继续尝试其他可读浏览器（优先推荐已登录的 Firefox/Edge 等）；
3. 所有自动来源都不可用时，显示可恢复的“无法自动读取浏览器会话”状态；
4. 只有用户主动选择时，才提供手动添加 Cookie 或 CLI/OAuth 备用方式。

没有浏览器 Cookie 不等于服务商已登录，也不能生成 100% 或其他静态成功值。

## 手动添加 Cookie（默认来源）

手动模式只做两件事：粘贴一个 Cookie 请求头，或导入符合大小限制的 Cookie 文件。
写入后存入现有受保护存储，前端、日志和诊断都不得显示原文。

默认来源就是手动：服务商读取用户保存的会话 Cookie；没有保存时不读浏览器，
回落到该服务商自己的登录阶梯。此外手动模式也覆盖以下情况：

- 用户明确选择「只用我保存的 Cookie」（与默认一致，显式固化）；
- 用户曾选「自动读取浏览器」但希望退回手动；
- provider 没有可用的自动来源，但官方会话 Cookie 仍可用。

手动添加不改变读取方式的选择状态，也不应把已保存 Cookie 误标为新的登录类型。

## 故障排查

- **Cookie 解密失败**：先关闭占用数据库的浏览器，再刷新；随后检查其他浏览器
  或使用 Firefox 作为自动读取回退。
- **读取为空**：确认普通浏览器中已登录目标服务商，且登录域名与 provider 域名一致。
- **WSL**：Chromium DPAPI 通常无法在 WSL 中解密；使用 Windows 宿主自动读取、手动
  添加 Cookie 或服务商 CLI/OAuth。

相关上游行为：[Win-CodexBar Cookie 说明](https://github.com/nesszer/Win-CodexBar/blob/main/docs/COOKIES.md)。
