# Browser Cookie Extraction（Windows）

本文是 V5 的认证与 Cookie 行为合同。全局默认来源与上游 Win-CodexBar 对齐
（2026-09-05 用户决策）：除小米 MiMo（Token Plan）例外采用浏览器优先外，
其他服务商默认只读取用户主动提供并保存的会话 Cookie；
自动读取浏览器会话是每个服务商的显式 opt-in，不默认开启。

`DEFAULT_COOKIE_SOURCE = "manual"`（`rust/src/settings.rs`）；上游保持同一默认
的原因：解密 Chromium Cookie 需要走 Windows DPAPI，可能触发杀毒软件告警，
浏览器读取必须由用户明确开启（MiMo 属于明确授权的特定例外）。

## 产品规则

- **手动是绝大多数服务商的默认来源**：对支持会话 Cookie 的服务商（除 MiMo 外），
  刷新默认读取用户粘贴或导入并保存的会话 Cookie；没有保存时回落到该服务商自己的登录阶梯（CLI/OAuth 等），
  不伪造成功值。
- **MiMo 浏览器优先例外**：小米 MiMo（Token Plan）未设置 `cookie_source` 时默认 `auto`（详见下文专用章节）。
- **自动读取通常是显式 opt-in**（MiMo 除外）：用户在其他服务商的「Cookie 来源」里选择「自动读取浏览器」后，
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

## 手动添加 Cookie（多数服务商的默认来源）

手动模式只做两件事：粘贴一个 Cookie 请求头，或导入符合大小限制的 Cookie 文件。
写入后存入现有受保护存储，前端、日志和诊断都不得显示原文。

MiMo 以外服务商的默认来源是手动：服务商读取用户保存的会话 Cookie；没有保存时不读浏览器，
回落到该服务商自己的登录阶梯。此外手动模式也覆盖以下情况：

- 用户明确选择「只用我保存的 Cookie」（与默认一致，显式固化）；
- 用户曾选「自动读取浏览器」但希望退回手动；
- provider 没有可用的自动来源，但官方会话 Cookie 仍可用。

手动添加不改变读取方式的选择状态，也不应把已保存 Cookie 误标为新的登录类型。

## Cookie 头缓存：浏览器锁定时的回退（2026-10-07）

所有"自动读取浏览器 Cookie"的供应商共用一层**按站点**的 Cookie 头缓存（DPAPI 加密存储，与手动 Cookie 同级保护）：

- 每次成功读取浏览器会话时自动刷新缓存；
- 浏览器运行期间，Chrome/Edge 对**正在使用的配置文件**的 Cookies 数据库持独占锁，实时读取会失败——此时若缓存未超过 24 小时，自动用缓存里的会话继续请求。**额度数字仍是实时请求的最新值**，被缓存的只是登录凭据；
- 浏览器里主动退出登录（提取明确报"该域无会话"）时缓存被清除并如实显示需要登录；
- 缓存命中在诊断与日志中如实标注，不会冒充实时读取。

含义：只要某供应商成功自动读取过一次，之后浏览器锁定期间卡片也能正常出数（最多滞后 24 小时的凭据新鲜度）。第一次读取仍需满足常规条件（浏览器未锁定该配置文件、或使用批量导入/手动粘贴）。

## MiMo 浏览器优先与回退规范（2026-09-29）

小米 MiMo（Token Plan）针对国内 Web 控制台会话特性，采用专有的浏览器优先（Browser-First）与已保存 Cookie 回退机制：

1. **默认来源隔离**：
   - MiMo 在配置未显式指定 `cookie_source` 时，默认解析为 `auto`（浏览器优先读取）。
   - 其他所有服务商严格保持全局 `DEFAULT_COOKIE_SOURCE = "manual"` 不变。
   - 用户显式持久化的配置（如 `manual` 或 `auto`）始终优先尊重，不被默认值重写。
2. **账号切换保护（Token 账号 Override）**：
   - 在 `auto` 模式下，若用户显式选择了特定的 MiMo Token 账号（`active_token_cookie`），系统直接使用该选定账号，**严禁探测浏览器本地存储**，防止多账号切换场景下被浏览器当前登录账号静默覆盖。
3. **获取顺序与回退（Ladder）**：
   - 无 Token 账号 override 时，优先尝试从本机受支持的浏览器读取当前 MiMo 会话 Cookie（需包含 `api-platform_serviceToken` 与 `userId`）。
   - **浏览器成功优先**：若浏览器 Cookie 获取有效且请求接口成功，直接采用该结果，绝不用陈旧的已保存 Cookie 替换。
   - **自动回退已保存 Cookie**：若浏览器 Cookie 获取失败（无可用会话）、校验失败（缺失必选键），或浏览器请求明确返回认证拒绝（HTTP 401/403 `AuthRequired`），系统自动回退尝试用户之前保存或导入的 MiMo Cookie。
   - 网络超时或服务端错误不触发账号切换；保留 provider 的真实错误或信息不足状态，不伪造额度。若认证回退的 Cookie 也失败，则返回它的真实错误。
4. **显式手动模式隔离**：
   - 用户显式选择 `cookie_source = "manual"` 时，仅使用已保存/导入的 Cookie，**绝不探测任何浏览器存储**。无已保存 Cookie 时直接返回 `NoCookies`。
5. **安全与隐私底线**：
   - 严禁打印、记录或泄露 Cookie 原始内容。
   - 不引入小米账号密码、短信验证码或 passToken 自动登录/刷新机制。

## 故障排查

- **Cookie 解密失败**：先关闭占用数据库的浏览器，再刷新；随后检查其他浏览器
  或使用 Firefox 作为自动读取回退。
- **读取为空**：确认普通浏览器中已登录目标服务商，且登录域名与 provider 域名一致。
- **WSL**：Chromium DPAPI 通常无法在 WSL 中解密；使用 Windows 宿主自动读取、手动
  添加 Cookie 或服务商 CLI/OAuth。

相关上游行为：[Win-CodexBar Cookie 说明](https://github.com/nesszer/Win-CodexBar/blob/main/docs/COOKIES.md)。
