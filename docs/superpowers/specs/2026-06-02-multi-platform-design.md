# 多平台支持 + 知乎真翻页 设计文档

日期：2026-06-02
项目：touch-fish（二进制 `moyu` / npm 包 `moyu-fish`）—— 伪装成 Claude Code 编码会话的终端摸鱼客户端。

## 1. 目标

1. **修复知乎推荐流重复**：当前 `recommend()` 始终请求同一个裸 `/recommend?action=down...` 地址，服务端返回的页之间大量重叠，导致刷新总看到看过的内容。真正的修复是跟随服务端返回的 `paging.next` 游标 URL 翻页（该 URL 携带 `session_token` 等服务端游标），每次重新签名其 path。现有的 `seen` HashSet 只是治标。
2. **新增 4 个平台**：V2EX、虎扑、NGA、Linux.do，采用与知乎一致的交互（列表 → 详情，全套 Claude Code 伪装）。

参考开源项目 `ylw1997/touchFish`（VSCode 扩展，TypeScript）的对应模块作为接口/抓取规则来源；本项目用 Rust 重新实现。

一次性发布（不分批），4 个平台全做。

## 2. 已确认的关键决策

- **架构**：方案 A —— 具体 `Platform` 枚举 + 各平台独立模块，worker 用 `match` 分发。不引入 async-trait / `dyn`，契合现有 `!Send` 单线程 worker。
- **帖子结构映射**：论坛帖子（主楼 + 楼层回复）全部拼进**一个**可滚动的详情正文，不拆成「正文 + 评论(Tab)」。
- **平台范围**：V2EX、虎扑（无需 cookie）+ NGA、Linux.do（各需独立 cookie）全做。
- **NGA 默认版块**：`fid=-7`（网事杂谈）作为落地版块。
- **新依赖**：允许新增 `encoding_rs`（NGA GBK 解码）与 `quick-xml`（NGA XML + Linux.do RSS）。HTML 抓取复用现有 `scraper`。

## 3. 架构

### 3.1 Platform 枚举与分发

```rust
pub enum Platform { Zhihu, V2ex, Hupu, Nga, LinuxDo }
```

- 每个平台一个模块（`src/platform/<name>/`），暴露**普通 async 函数**：
  - `list(cursor) -> Result<(Vec<ListEntry>, Cursor)>`
  - `detail(token) -> Result<Vec<DetailView>>`
- worker 持有一个 `Sources` 结构：懒构建的各平台 client + 每平台的当前 `Cursor`。收到请求时 `match` 当前活跃平台，调用对应模块函数。
- 知乎仍走签名 JSON API；论坛走明文 GET（NGA 走 GBK 字节）。

### 3.2 通用视图模型（src/platform/mod.rs）

- `ListEntry`：保留 `title` / `subtitle` / `detail: Option<DetailView>`（预取，知乎推荐卡片用）；将 `question_id: Option<String>` 泛化为 `open_token: Option<String>`（知乎=问题 id；论坛=帖子 URL；NGA=tid）。
- `DetailView`：字段不变（`author` / `voteup` / `body` / `images` / `answer_id`）。论坛语义：`voteup`→回复数（无则 0）；`answer_id`→帖子 id（继续用作图片缓存 owner key）。

### 3.3 翻页 / 去重

worker 为每个平台保存一个 `Cursor`：

- **知乎推荐** → cursor = `paging.next` URL；在 `RecommendResponse` 增加 `paging { next: Option<String>, is_end: bool }`；翻页时取 `next` 的 path 重新签名请求。**这是重复问题的真正修复。**
- **NGA** → cursor = 页码，刷新时 +1。
- **V2EX / 虎扑 / Linux.do** → 无服务端游标；刷新即重新抓取。

通用兜底：现有会话级 `seen` HashSet 从「仅推荐」提升为**对所有平台的所有列表更新生效**的重复过滤层。知乎因此同时获得「游标翻页 + 去重」双保险。`App::apply_recommend` 泛化为 `apply_list_deduped`（对任意来源去重；整批都见过则保留当前列表不致白屏）。平台切换时清空/重置 `seen` 与 cursor。

### 3.4 网络层（src/net/mod.rs）

- 保留 `signed_get`（知乎签名请求）与 `fetch_bytes`（图片）。
- 新增通用 `get_text(url, headers) -> String` 与 `get_bytes(url, headers) -> Vec<u8>`（NGA 用后者拿 GBK 字节再用 `encoding_rs` 解码）。
- `HOST` 常量保留给知乎；论坛用各自完整 URL。

## 4. 各平台模块

### 4.1 V2EX（无 cookie）
- 列表：GET `https://www.v2ex.com/?tab=all` → `scraper` 选 `#Main .box` 首块的 `.cell.item .topic-link`，取标题与 `href`（去掉 `#`/`?` 后缀），`open_token` = 帖子相对 URL。
- 详情：GET `https://www.v2ex.com{url}`，取 `#Main` 的 HTML（移除标题 `h1` 避免重复）→ `to_text_and_images`。

### 4.2 虎扑（无 cookie）
- 列表：GET `https://bbs.hupu.com/all-gambia` → `.text-list-model .list-item a .t-title` 标题 + `href`。
- 详情：GET 帖子页（去掉 `-N.html` 页码后缀），先用正则去掉 CSS 动态混淆串（`__x"` / `__x ` 模式），再取 `.index_bbs-post-web-body-left-wrapper` → `to_text_and_images`。

### 4.3 NGA（需 cookie）—— 最重
- 列表：GET `https://bbs.nga.cn/thread.php?fid=-7&page=N&lite=xml`，`responseType=bytes` → `encoding_rs` GBK 解码 → `quick-xml` 解析 `__T` 下的 `item`：`tid` / `subject` / `replies`，标题展示为 `[回复数] 标题`，`open_token` = `/read.php?tid={tid}`。
- 详情：GET `https://bbs.nga.cn/read.php?tid=X&lite=xml`（GBK），解析主楼 + 各楼回复，按楼层顺序拼成一个正文。内容做基础清洗：NGA 图片 `[img]./xxx[/img]` → 真实 URL（`img.nga.178.com/...`，记入 `images`，用 `【图N】` 标记，与知乎一致）；表情 `[s:xx:yy]` → 文字占位或删除；`[quote]`/`[url]` 等 BBCode → 纯文本。范围限定为可读的纯文本子集，不追求 100% 还原 874 行参考实现。
- cookie 缺失：列表返回一条「NGA 未配置 cookie（回车去配置）」的占位项，回车进入登录流程。

### 4.4 Linux.do（需 cookie）
- 列表：GET `https://linux.do/latest.rss`（带 cookie + 浏览器风格头）→ `quick-xml` 解析 `<item>`：`title` / `link` / `pubDate` / `dc:creator`，`open_token` = 帖子 URL。
- 详情：GET `{url}.rss` → 解析所有 `<item>`（RSS 倒序，最后一条为主楼）：主楼正文 + 其余楼层回复拼成一个正文。清理「阅读完整话题」尾链。
- cookie 缺失：同 NGA，占位项 → 登录流程。

### 4.5 共享 HTML 工具
- 将 `to_text_and_images` 从 `zhihu::html` 提升为 `platform::html`（或在 `platform` 顶层 re-export），供所有平台复用。

## 5. 配置与登录

- `Config` 新增 `nga.cookie`、`linuxdo.cookie`（保持 `zhihu.cookie`）。
- 复用现有 Login 屏幕。切换到需要 cookie 的平台且本地无 cookie 时，路由到 Login，并标记目标平台；回车后将输入存入该平台的 cookie 并连接。`App` 记录「待登录的目标平台」。

## 6. UI / 伪装

完全复用现有能力，均已对 `ListEntry` / `DetailView` 泛化：

- Todo-list 列表皮肤（`● Update Todos` + 复选框 + 分组间穿插 decoy 代码块）。
- 老板键、伪装开关 `c`、一行简介截断、图片下载 + 数字键在编辑器打开。
- 论坛详情是单个较长的 `DetailView`：只有一条时禁用 `n/p`；`Tab`/评论页仍仅知乎可用（其它平台为 no-op）。
- 标题/状态栏显示当前活跃平台名。

## 7. 命令

- 新增 `/v2ex`、`/hupu`、`/nga`、`/linuxdo`：各自设置活跃平台并加载默认板块/feed。
- `/zhihu` 不变；`/hot`、`/search` 仍仅知乎可用（其它平台下提示不支持或忽略）。
- `ListSource` 扩展为携带 `Platform`，刷新/翻页据此路由。

## 8. 错误处理

- 网络/解析失败：沿用现有 `Update::Error(String)`，在状态栏显示，不导航。
- cookie 缺失/失效（NGA、Linux.do）：列表占位项引导到登录；403/401 给出「cookie 失效，请重新配置」提示。
- 单平台失败不影响其它平台已加载的状态。

## 9. 测试

- **解析单测**：每个平台用一小段保存的 HTML/XML/RSS 夹具 → 断言 `ListEntry` / `DetailView` 字段（标题、token、回复数、图片提取、楼层拼接）。
- **翻页单测**：`paging` 解析（知乎）；cursor 前进（知乎 next-url、NGA 页码 +1）。
- **去重单测**：泛化后的 `apply_list_deduped` 跨平台去重、整批见过保留列表。
- **命令分发**：`/v2ex` 等设置正确 `Platform` 并发出对应请求；cookie-gated 平台无 cookie 时路由到 Login 且标记目标平台。
- **`#[ignore]` 实网测试**：每个平台一条（参照现有知乎 live 测试），从 env/config 取 cookie。

## 10. 非目标（YAGNI）

- 不做：发帖/点赞/评论提交、登录态自动续期、NGA BBCode 100% 还原、论坛多 tab/多版块切换 UI（NGA 仅默认 fid，后续可加 `/nga <fid>`）、知乎以外平台的搜索/热榜。
