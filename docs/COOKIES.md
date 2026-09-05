# Browser Cookie Extraction（Windows）

本文是 V5 的认证与 Cookie 行为合同。目标是恢复 CodexBar 原本的使用方式：
启用服务商后，默认自动读取当前 Windows 用户已登录的浏览器会话；用户不需要
复制 Cookie，也不需要在 TokenBar 内重新登录。

本文描述的是当前确认的目标行为；运行时代码尚未因本次文档同步而改变，执行者必须
按本文和 `AGENT_HANDOFF.md` 当前合同完成实现并提供真实 Windows 验收证据；`CURRENT_TASK.md`
仅为兼容性路由指针。

## 产品规则

- **自动是默认路径**：对启用且支持网页会话的服务商，刷新时自动寻找可用浏览器
  Cookie，并在内存中组装请求头。
- **API 密钥是另一类凭据**：只有服务商真实支持 API 时才显示，不与 Cookie 读取
  位置混成一个登录分段。
- **CLI/OAuth 是服务商专用登录动作**：只有服务商确实支持时才显示。Codex 的
  CLI/OAuth 登录不能被网页 Cookie 登录替代；Codex 的 Cookie 只用于网页数据补充。
- **手动添加 Cookie 保留**：粘贴 Cookie 请求头或批量导入文件是后加的兼容/故障
  兜底模式，默认收在高级区域，不得成为首次使用的前置步骤。
- **应用内 WebView 登录不属于正常路径**：打开网页、捕获 WebView2 Cookie 的旁路
  不得作为“自动登录”或网页会话的主入口；若旧版本保留命令，只能作为迁移兼容，
  不能让用户必须走它。

## 自动读取范围

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

正常刷新不显示“选择浏览器”“复制 Cookie”或“打开网页登录”步骤。

## App-Bound Encryption 与失败回退

Chrome/Edge 新版可能使用 App-Bound Encryption（例如 v20 Cookie），或者浏览器
正在锁定数据库。自动流程必须：

1. 记录本次浏览器和失败原因，但不把失败伪装成成功额度；
2. 继续尝试其他可读浏览器（优先推荐已登录的 Firefox/Edge 等）；
3. 所有自动来源都不可用时，显示可恢复的“无法自动读取浏览器会话”状态；
4. 只有用户主动选择时，才提供手动添加 Cookie 或 CLI/OAuth 备用方式。

没有浏览器 Cookie 不等于服务商已登录，也不能生成 100% 或其他静态成功值。

## 手动添加 Cookie（保留的后加模式）

手动模式只做两件事：粘贴一个 Cookie 请求头，或导入符合大小限制的 Cookie 文件。
写入后存入现有受保护存储，前端、日志和诊断都不得显示原文。

手动模式仅在以下情况使用：

- 自动读取因 App-Bound Encryption、权限、数据库锁或 WSL 环境失败；
- 用户明确选择“只使用我保存的 Cookie”；
- provider 没有可用的自动来源，但官方网页会话仍可用。

手动添加不改变自动默认，也不应把已保存 Cookie 误标为新的登录类型。

## 故障排查

- **Cookie 解密失败**：先关闭占用数据库的浏览器，再刷新；随后检查其他浏览器
  或使用 Firefox 作为自动读取回退。
- **读取为空**：确认普通浏览器中已登录目标服务商，且登录域名与 provider 域名一致。
- **WSL**：Chromium DPAPI 通常无法在 WSL 中解密；使用 Windows 宿主自动读取、手动
  添加 Cookie 或服务商 CLI/OAuth。

相关上游行为：[Win-CodexBar Cookie 说明](https://github.com/nesszer/Win-CodexBar/blob/main/docs/COOKIES.md)。
