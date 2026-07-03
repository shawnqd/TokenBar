# AGENTS

## 目标

本仓库用于把 `onWatch` 改造成个人本地双端使用的「模型额度看板」。

执行目标不是重写系统，而是在不破坏 `onWatch` 原能力的前提下，先交付 P0：

- 平台管理
- 套餐管理
- 额度桶管理
- 当前模型
- 到期 / 重置时间
- API Key 状态
- Base URL
- 风险备注
- 首页推荐
- 导入 / 导出

## 先读顺序

1. `docs/CODE_HANDOFF.md`
2. `docs/PRODUCT_PLAN.md`
3. `docs/TECHNICAL_MANUAL.md`
4. `docs/MAINTENANCE_MANUAL.md`
5. 当前底座代码的 `README.md`
6. `docs/API_INTEGRATIONS_SETUP.md`

## 当前工作方式

### 只做

- P0
- 手动管理优先
- 新表、新页面、新路由
- 只读配置检测
- 脱敏导入导出

### 不做

- P1 Provider 自动适配
- 保存真实密钥
- Cookie 持久化
- 网页余额抓取
- 云同步
- 多用户

## 实施原则

1. 先审查再改造
2. 采集层和台账层分离
3. 原 `onWatch` Provider 路径不轻易改语义
4. 优先新增独立模块，不把台账逻辑散落到旧采集逻辑
5. 单个自动化失败不能影响手动台账

## 停手规则

出现以下情况必须停下并更新交接文件，不要擅自扩大范围：

1. 必须破坏旧 Provider 才能完成 P0
2. 必须保存真实 API Key / Cookie 才能继续
3. 页面必须重写大量底座结构才可落地
4. 真实代码结构与文档假设差异过大

## 更新规则

每完成一个包，至少更新：

- `PROJECT_STATUS.md`
- `TODO.md`
- `CHANGELOG.md`
- `AGENT_HANDOFF.md`

如果发现纯技术问题，也直接写进 `AGENT_HANDOFF.md`，不要把用户当传话中间层。
