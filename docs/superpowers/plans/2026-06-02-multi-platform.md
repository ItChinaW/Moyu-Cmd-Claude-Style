# 多平台 + 知乎真翻页 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复知乎推荐重复(跟随 `paging.next` 游标真翻页),并新增 V2EX / 虎扑 / NGA / Linux.do 四个平台,沿用知乎的列表→详情交互与 Claude Code 伪装。

**Architecture:** 具体 `Platform` 枚举 + 各平台独立模块,worker 用 `match` 分发(不引 async-trait / `dyn`)。视图模型 `ListEntry` / `DetailView` 泛化复用;会话级 `seen` 去重提升为全平台兜底;知乎额外用 `paging.next` 游标翻页。论坛帖子「主楼+回复」拼成单页可滚动正文。

**Tech Stack:** Rust, tokio(current-thread worker), reqwest(rustls), scraper(HTML), 新增 `quick-xml`(NGA XML + RSS) 与 `encoding_rs`(NGA GBK)。ratatui/crossterm TUI。

**Spec:** `docs/superpowers/specs/2026-06-02-multi-platform-design.md`

---

## File Structure

- `Cargo.toml` — 新增 `quick-xml`、`encoding_rs` 依赖。
- `src/net/mod.rs` — 新增 `get_text(url, headers)` / `get_bytes(url, headers)`。
- `src/platform/mod.rs` — `Platform` 枚举;`ListEntry.question_id` → `open_token`;`platform::html` re-export。
- `src/platform/html.rs` — 由 `zhihu/html.rs` 提升的共享 HTML→文本工具(NEW;`zhihu/html.rs` 改为 re-export)。
- `src/platform/zhihu/model.rs` — `RecommendResponse` 增加 `paging`。
- `src/platform/zhihu/client.rs` — `recommend(cursor)` 返回 `(entries, next_cursor)`;`open_token` 重命名。
- `src/platform/v2ex/mod.rs` — V2EX list/detail(NEW)。
- `src/platform/hupu/mod.rs` — 虎扑 list/detail(NEW)。
- `src/platform/nga/mod.rs` — NGA list/detail(GBK+XML)(NEW)。
- `src/platform/linuxdo/mod.rs` — Linux.do list/detail(RSS)(NEW)。
- `src/config/mod.rs`(或现 config 文件)— 新增 `nga.cookie` / `linuxdo.cookie`。
- `src/app/mod.rs` — `App.active_platform`、`pending_login_platform`;`apply_recommend` → `apply_list_deduped`;`entry_key` 用 `open_token`。
- `src/app/runner.rs` — 泛化 `Request`/`Update`;`Sources` 持有各 client + per-platform cursor;命令分发与 worker `match`。
- `src/app/command.rs` — 新增 `/v2ex /hupu /nga /linuxdo` 命令。
- `src/ui/screens.rs` — 状态/标题显示当前平台名(其余复用)。
- `tests/fixtures/` — 各平台抓取样本(NEW)。

---

## Phase 0 — 基础设施

### Task 0.1: 新增依赖

**Files:** Modify `Cargo.toml`

- [ ] **Step 1: 在 `[dependencies]` 末尾追加**

```toml
quick-xml = "0.36"
encoding_rs = "0.8"
```

- [ ] **Step 2: 验证编译**

Run: `cargo build`
Expected: 编译通过(新依赖下载成功)。

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build: add quick-xml + encoding_rs for NGA/RSS parsing"
```

### Task 0.2: net 通用 GET

**Files:** Modify `src/net/mod.rs`

- [ ] **Step 1: 写失败测试(放入 `src/net/mod.rs` 的 `mod tests`)**

```rust
    #[tokio::test]
    #[ignore = "live network; run with --ignored"]
    async fn get_text_fetches_v2ex() {
        let c = HttpClient::new().unwrap();
        let html = c.get_text("https://www.v2ex.com/?tab=all", &[]).await.unwrap();
        assert!(html.contains("<"), "should return HTML");
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib net:: -- --ignored get_text_fetches_v2ex`
Expected: 编译失败(`get_text` 不存在)。

- [ ] **Step 3: 实现 `get_text` / `get_bytes`(加到 `impl HttpClient`)**

```rust
    /// GET an arbitrary URL with optional extra headers, return body text.
    pub async fn get_text(&self, url: &str, headers: &[(&str, &str)]) -> Result<String> {
        let mut req = self.inner.get(url).header("user-agent", USER_AGENT);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = req.send().await.context("send get_text")?;
        let status = resp.status();
        let body = resp.text().await.context("read get_text body")?;
        if !status.is_success() {
            anyhow::bail!("HTTP {status}: {}", body.chars().take(200).collect::<String>());
        }
        Ok(body)
    }

    /// GET an arbitrary URL with optional extra headers, return raw bytes (for GBK pages).
    pub async fn get_bytes(&self, url: &str, headers: &[(&str, &str)]) -> Result<Vec<u8>> {
        let mut req = self.inner.get(url).header("user-agent", USER_AGENT);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = req.send().await.context("send get_bytes")?;
        let status = resp.status();
        let bytes = resp.bytes().await.context("read get_bytes")?;
        if !status.is_success() {
            anyhow::bail!("HTTP {status} fetching bytes");
        }
        Ok(bytes.to_vec())
    }
```

- [ ] **Step 4: 编译验证**

Run: `cargo build`
Expected: 通过。

- [ ] **Step 5: Commit**

```bash
git add src/net/mod.rs
git commit -m "feat(net): generic get_text/get_bytes for non-zhihu platforms"
```

### Task 0.3: 共享 HTML 工具提升

**Files:** Create `src/platform/html.rs`; Modify `src/platform/zhihu/html.rs`, `src/platform/mod.rs`, `src/platform/zhihu/mod.rs`

- [ ] **Step 1: 把 `src/platform/zhihu/html.rs` 整个文件内容移动到 `src/platform/html.rs`**

```bash
git mv src/platform/zhihu/html.rs src/platform/html.rs
```

- [ ] **Step 2: 在 `src/platform/mod.rs` 顶部声明模块**

加一行(在 `pub mod zhihu;` 之前):

```rust
pub mod html;
```

- [ ] **Step 3: 在 `src/platform/zhihu/mod.rs` 中把 `html` 改为 re-export 父模块**

将原 `pub mod html;`(或类似声明)替换为:

```rust
pub use crate::platform::html;
```

- [ ] **Step 4: 运行现有 html 测试**

Run: `cargo test --lib html`
Expected: 原有 `to_text_and_images` 测试 PASS(路径变更后仍通过)。

- [ ] **Step 5: 全量编译 + 测试**

Run: `cargo test --lib`
Expected: 全绿(53 项左右)。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: promote html to_text_and_images to platform::html (shared)"
```

---

## Phase 1 — 知乎真翻页(重复修复)

### Task 1.1: RecommendResponse 解析 paging

**Files:** Modify `src/platform/zhihu/model.rs`

- [ ] **Step 1: 写失败测试(加到 model.rs 的 `mod tests`)**

```rust
    #[test]
    fn parses_recommend_paging_next() {
        let raw = r#"{"data":[],"paging":{"is_end":false,"next":"https://www.zhihu.com/api/v3/feed/topstory/recommend?session_token=abc&after_id=5&action=down&desktop=true"}}"#;
        let r: RecommendResponse = serde_json::from_str(raw).expect("parse");
        let p = r.paging.expect("paging present");
        assert!(!p.is_end);
        assert!(p.next.unwrap().contains("session_token=abc"));
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib parses_recommend_paging_next`
Expected: 编译失败(`paging` 字段不存在)。

- [ ] **Step 3: 在 `RecommendResponse` 增加字段并定义 `Paging`**

`RecommendResponse` 改为:

```rust
#[derive(Debug, Deserialize)]
pub struct RecommendResponse {
    pub data: Vec<RecommendItem>,
    #[serde(default)]
    pub paging: Option<Paging>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Paging {
    #[serde(default)]
    pub is_end: bool,
    #[serde(default)]
    pub next: Option<String>,
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --lib parses_recommend_paging_next`
Expected: PASS。同时 `cargo test --lib` 全绿(已有 recommend 测试不受影响,因 `paging` 为 `Option`+default)。

- [ ] **Step 5: Commit**

```bash
git add src/platform/zhihu/model.rs
git commit -m "feat(zhihu): parse recommend paging.next cursor"
```

### Task 1.2: recommend 跟随游标翻页

**Files:** Modify `src/platform/zhihu/client.rs`

- [ ] **Step 1: 写失败测试(加到 client.rs 的 `mod tests`,纯函数 helper)**

新增一个纯函数 `recommend_path(cursor: Option<&str>) -> String`,并测它:

```rust
    #[test]
    fn recommend_path_uses_cursor_when_present() {
        // 无游标 → 默认起始路径
        assert_eq!(
            recommend_path(None),
            "/api/v3/feed/topstory/recommend?action=down&ad_interval=-1&desktop=true"
        );
        // 有游标(完整 next URL)→ 取其 path+query
        let next = "https://www.zhihu.com/api/v3/feed/topstory/recommend?session_token=abc&after_id=5&action=down&desktop=true";
        assert_eq!(
            recommend_path(Some(next)),
            "/api/v3/feed/topstory/recommend?session_token=abc&after_id=5&action=down&desktop=true"
        );
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib recommend_path_uses_cursor_when_present`
Expected: 编译失败(`recommend_path` 未定义)。

- [ ] **Step 3: 实现 `recommend_path` 并改造 `recommend` 返回游标**

在 client.rs 文件级新增纯函数:

```rust
/// Path+query for a recommend request: the server-provided `next` URL's path when
/// paging, else the default first-page path.
fn recommend_path(cursor: Option<&str>) -> String {
    match cursor {
        Some(next) if next.starts_with("http") => {
            // strip scheme+host, keep path?query
            match next.find("/api/") {
                Some(i) => next[i..].to_string(),
                None => next.to_string(),
            }
        }
        _ => "/api/v3/feed/topstory/recommend?action=down&ad_interval=-1&desktop=true".to_string(),
    }
}
```

把 `recommend` 改为接受游标、返回下一个游标:

```rust
    pub async fn recommend(&self, cursor: Option<&str>) -> Result<(Vec<ListEntry>, Option<String>)> {
        let body = self.get(&recommend_path(cursor)).await?;
        let resp: model::RecommendResponse = serde_json::from_str(&body)?;
        let next = resp.paging.and_then(|p| if p.is_end { None } else { p.next });
        let entries = resp.data.into_iter().filter_map(|item| {
            let target = item.target;
            match target.kind.as_str() {
                "answer" => {
                    let q = target.question?;
                    if q.title.is_empty() { return None; }
                    let open_token = if q.id.is_empty() { None } else { Some(q.id) };
                    let detail = if target.content.is_empty() {
                        None
                    } else {
                        let (body, images) = html::to_text_and_images(&target.content);
                        Some(DetailView {
                            author: target.author.name,
                            voteup: target.voteup_count,
                            body, images,
                            answer_id: target.id,
                        })
                    };
                    Some(ListEntry { title: q.title, subtitle: target.excerpt, open_token, detail })
                }
                "article" => {
                    let title = target.title;
                    if title.is_empty() { return None; }
                    Some(ListEntry { title, subtitle: target.excerpt, open_token: None, detail: None })
                }
                _ => None,
            }
        }).collect();
        Ok((entries, next))
    }
```

> 注:此处已用 `open_token`(在 Phase 2 Task 2.1 完成字段重命名)。若先做本任务,临时用 `question_id`,Task 2.1 再统一改名;推荐先做 Task 2.1 再回此步。**执行顺序:先 Task 2.1(改名),再 1.2 的本步。**

- [ ] **Step 4: 更新 `Connect` 与 live 测试调用点**

`recommend()` 的两处调用(`handle` 中 Connect 分支、`recommend` 分支)与 live 测试 `client.recommend()` → `client.recommend(None)`(取 `.0`)。worker 侧改造在 Phase 2,这里仅让 live 测试可编译:

```rust
        let (results, _next) = client.recommend(None).await.expect("recommend");
```

- [ ] **Step 5: 运行测试**

Run: `cargo test --lib recommend_path_uses_cursor_when_present`
Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add src/platform/zhihu/client.rs
git commit -m "feat(zhihu): recommend follows paging.next cursor (real pagination)"
```

---

## Phase 2 — 平台抽象

### Task 2.1: ListEntry.question_id → open_token

**Files:** Modify `src/platform/mod.rs`, `src/app/mod.rs`, `src/app/runner.rs`, `src/app/state.rs`, `src/ui/mod.rs`, `src/platform/zhihu/client.rs`

机械重命名:把 `ListEntry` 字段 `question_id` 改为 `open_token`(语义不变,泛化命名)。

- [ ] **Step 1: 改 `src/platform/mod.rs`**

```rust
    /// Token used to open this row: Zhihu question id, or a forum topic URL/tid.
    pub open_token: Option<String>,
```

- [ ] **Step 2: 全仓替换字段名**

逐个文件把 `question_id:`(结构体字面量/字段访问)改为 `open_token:`,涉及:
- `src/app/mod.rs:16` `entry_key`:`if let Some(q) = &e.question_id` → `&e.open_token`。
- `src/app/runner.rs`:`e.question_id.clone()`(403)、`question_id: qid...`(486)、测试 764 字面量、`entry()` helper。
- `src/app/state.rs:18`、`src/ui/mod.rs:48-49` 字面量。
- `src/platform/zhihu/client.rs`:`hot_list`(37)、`search`(56-57)、`recommend`(已在 1.2 用 open_token)、live 测试里 `e.question_id` → `e.open_token`。
  - 注意:`answers(&self, question_id: &str)` 的**函数参数**名保留(那是知乎问题 id 语义,不是结构体字段),不改。

- [ ] **Step 3: 编译 + 测试**

Run: `cargo test --lib`
Expected: 全绿。

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: ListEntry.question_id -> open_token (platform-generic)"
```

### Task 2.2: Platform 枚举 + App 状态

**Files:** Modify `src/platform/mod.rs`, `src/app/mod.rs`

- [ ] **Step 1: 写失败测试(加到 `src/app/mod.rs` 的 tests,文件末尾若无则新建 `mod tests`)**

```rust
    #[test]
    fn default_platform_is_zhihu() {
        let app = App::new();
        assert_eq!(app.active_platform, crate::platform::Platform::Zhihu);
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib default_platform_is_zhihu`
Expected: 编译失败。

- [ ] **Step 3: 定义 `Platform`(`src/platform/mod.rs` 顶部)**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform { Zhihu, V2ex, Hupu, Nga, LinuxDo }

impl Platform {
    /// Human label shown in the (non-camouflaged) status line.
    pub fn label(self) -> &'static str {
        match self {
            Platform::Zhihu => "知乎",
            Platform::V2ex => "V2EX",
            Platform::Hupu => "虎扑",
            Platform::Nga => "NGA",
            Platform::LinuxDo => "Linux.do",
        }
    }
    /// Whether this platform needs a user-supplied cookie.
    pub fn needs_cookie(self) -> bool {
        matches!(self, Platform::Zhihu | Platform::Nga | Platform::LinuxDo)
    }
}
```

- [ ] **Step 4: 在 `App` 增加字段**

`struct App` 增加:

```rust
    pub active_platform: crate::platform::Platform,
    /// When a cookie-gated platform was requested without a stored cookie, the
    /// platform the pending Login screen should connect once a cookie is entered.
    pub pending_login_platform: Option<crate::platform::Platform>,
```

`App::new()` 初始化:

```rust
            active_platform: crate::platform::Platform::Zhihu,
            pending_login_platform: None,
```

- [ ] **Step 5: 运行测试**

Run: `cargo test --lib default_platform_is_zhihu`
Expected: PASS。`cargo test --lib` 全绿。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: Platform enum + App.active_platform/pending_login_platform"
```

### Task 2.3: 去重泛化 apply_recommend → apply_list_deduped

**Files:** Modify `src/app/mod.rs`, `src/app/runner.rs`

- [ ] **Step 1: 改测试名与语义(`src/app/runner.rs` 现有 `recommend_dedup_skips_seen_rows`)**

把该测试里的 `app.apply_recommend(...)` 调用全部改为 `app.apply_list_deduped(...)`,断言不变。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib recommend_dedup_skips_seen_rows`
Expected: 编译失败(`apply_list_deduped` 未定义)。

- [ ] **Step 3: 重命名方法 + 增加平台切换重置**

`src/app/mod.rs`:把 `pub fn apply_recommend` 改名为 `pub fn apply_list_deduped`(实现不变)。新增:

```rust
    /// Switch the active platform: reset dedup memory and current list so the new
    /// platform starts clean.
    pub fn switch_platform(&mut self, p: crate::platform::Platform) {
        if self.active_platform != p {
            self.active_platform = p;
            self.seen.clear();
            self.list.clear();
            self.list_cursor = 0;
        }
    }
```

- [ ] **Step 4: 更新 runner 调用点**

`src/app/runner.rs` 中 `apply_update` 的 `Update::Connected` 与 `Update::List` 两处 `app.apply_recommend(list)` → `app.apply_list_deduped(list)`。`Update::List` 分支去掉 `if list_source == Recommend` 判断,改为**所有**列表都走 `apply_list_deduped`(统一全平台去重)。

`Update::List` 改为:

```rust
        Update::List(list) => {
            app.error = None;
            app.apply_list_deduped(list);
            match app.screen() {
                Screen::List => {}
                Screen::Login => app.replace(Screen::List),
                _ => app.push(Screen::List),
            }
        }
```

- [ ] **Step 5: 运行测试**

Run: `cargo test --lib`
Expected: 全绿。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: generalize dedup to all platforms (apply_list_deduped + switch_platform)"
```

### Task 2.4: 泛化 Request/Update + Sources(worker 改造)

**Files:** Modify `src/app/runner.rs`

这是核心改造。`Request`/`Update` 改为平台无关,worker 持有 `Sources`(各 client + per-platform cursor),按 `Platform` 分发。

- [ ] **Step 1: 定义新的 Request/Update**

替换现有枚举:

```rust
#[derive(Debug)]
pub enum Request {
    /// Connect/verify a cookie for a platform, then load its default list.
    Connect { platform: Platform, cookie: String },
    /// Load the default/first page for a platform (resets cursor).
    List(Platform),
    /// Load the next page (uses stored cursor); falls back to first page.
    More(Platform),
    /// Zhihu-only: hot list.
    HotList,
    /// Zhihu-only: search.
    Search(String),
    /// Open a row: fetch its detail(s) given the open token.
    Detail { platform: Platform, token: String },
    /// Zhihu-only: comments for an answer id.
    Comments(String),
    FetchImages { answer_id: String, urls: Vec<String> },
}

pub enum Update {
    Connected { platform: Platform, cookie: String, list: Vec<ListEntry> },
    ConnectFailed(String),
    List(Vec<ListEntry>),
    Details(Vec<DetailView>),
    Comments(Vec<CommentView>),
    ImagesReady { answer_id: String, paths: Vec<String> },
    Error(String),
}
```

`use` 增加:`use crate::platform::Platform;`

- [ ] **Step 2: 定义 Sources 并改 worker 循环**

替换 `spawn_worker` 内的 `let mut client: Option<ZhihuClient>` 模型:

```rust
#[derive(Default)]
struct Sources {
    zhihu: Option<ZhihuClient>,
    zhihu_cursor: Option<String>,
    nga_cookie: String,
    nga_page: u32,
    linuxdo_cookie: String,
    http: Option<crate::net::HttpClient>,
}

impl Sources {
    fn http(&mut self) -> crate::net::HttpClient {
        if self.http.is_none() {
            self.http = crate::net::HttpClient::new().ok();
        }
        self.http.clone().expect("http client")
    }
}
```

worker 循环:

```rust
        rt.block_on(async move {
            let mut src = Sources::default();
            while let Some(req) = rx.recv().await {
                let upd = handle(&mut src, req).await;
                if tx.send(upd).is_err() { break; }
            }
        });
```

- [ ] **Step 3: 重写 `handle`**

```rust
async fn handle(src: &mut Sources, req: Request) -> Update {
    match req {
        Request::Connect { platform, cookie } => connect(src, platform, cookie).await,
        Request::List(p) => { reset_cursor(src, p); load_list(src, p).await }
        Request::More(p) => load_list(src, p).await,
        Request::HotList => match &src.zhihu {
            Some(c) => match c.hot_list().await { Ok(v) => Update::List(v), Err(e) => Update::Error(e.to_string()) },
            None => Update::Error("未登录知乎".into()),
        },
        Request::Search(q) => match &src.zhihu {
            Some(c) => match c.search(&q).await { Ok(v) => Update::List(v), Err(e) => Update::Error(e.to_string()) },
            None => Update::Error("未登录知乎".into()),
        },
        Request::Detail { platform, token } => load_detail(src, platform, &token).await,
        Request::Comments(id) => match &src.zhihu {
            Some(c) => match c.comments(&id).await { Ok(v) => Update::Comments(v), Err(e) => Update::Error(e.to_string()) },
            None => Update::Error("未登录知乎".into()),
        },
        Request::FetchImages { answer_id, urls } => {
            let http = src.http();
            let paths = download_images(&http, &urls).await;
            Update::ImagesReady { answer_id, paths }
        }
    }
}

fn reset_cursor(src: &mut Sources, p: Platform) {
    match p { Platform::Zhihu => src.zhihu_cursor = None, Platform::Nga => src.nga_page = 1, _ => {} }
}

async fn connect(src: &mut Sources, platform: Platform, cookie: String) -> Update {
    match platform {
        Platform::Zhihu => match ZhihuClient::new(cookie.clone()) {
            Ok(c) => match c.recommend(None).await {
                Ok((list, next)) => { src.zhihu = Some(c); src.zhihu_cursor = next;
                    Update::Connected { platform, cookie, list } }
                Err(e) => Update::ConnectFailed(e.to_string()),
            },
            Err(e) => Update::ConnectFailed(e.to_string()),
        },
        Platform::Nga => { src.nga_cookie = cookie.clone(); src.nga_page = 1;
            let http = src.http();
            match crate::platform::nga::list(&http, &src.nga_cookie, 1).await {
                Ok(list) => Update::Connected { platform, cookie, list },
                Err(e) => Update::ConnectFailed(e.to_string()),
            }
        }
        Platform::LinuxDo => { src.linuxdo_cookie = cookie.clone();
            let http = src.http();
            match crate::platform::linuxdo::list(&http, &src.linuxdo_cookie).await {
                Ok(list) => Update::Connected { platform, cookie, list },
                Err(e) => Update::ConnectFailed(e.to_string()),
            }
        }
        _ => Update::Error("该平台无需登录".into()),
    }
}

async fn load_list(src: &mut Sources, p: Platform) -> Update {
    let http = src.http();
    match p {
        Platform::Zhihu => match &src.zhihu {
            Some(c) => match c.recommend(src.zhihu_cursor.as_deref()).await {
                Ok((list, next)) => { src.zhihu_cursor = next; Update::List(list) }
                Err(e) => Update::Error(e.to_string()),
            },
            None => Update::Error("未登录知乎".into()),
        },
        Platform::V2ex => match crate::platform::v2ex::list(&http).await {
            Ok(v) => Update::List(v), Err(e) => Update::Error(e.to_string()) },
        Platform::Hupu => match crate::platform::hupu::list(&http).await {
            Ok(v) => Update::List(v), Err(e) => Update::Error(e.to_string()) },
        Platform::Nga => {
            let page = src.nga_page;
            let r = crate::platform::nga::list(&http, &src.nga_cookie, page).await;
            match r { Ok(v) => { src.nga_page = page + 1; Update::List(v) } Err(e) => Update::Error(e.to_string()) }
        }
        Platform::LinuxDo => match crate::platform::linuxdo::list(&http, &src.linuxdo_cookie).await {
            Ok(v) => Update::List(v), Err(e) => Update::Error(e.to_string()) },
    }
}

async fn load_detail(src: &mut Sources, p: Platform, token: &str) -> Update {
    let http = src.http();
    let r = match p {
        Platform::Zhihu => match &src.zhihu {
            Some(c) => c.answers(token).await,
            None => Err(anyhow::anyhow!("未登录知乎")),
        },
        Platform::V2ex => crate::platform::v2ex::detail(&http, token).await,
        Platform::Hupu => crate::platform::hupu::detail(&http, token).await,
        Platform::Nga => crate::platform::nga::detail(&http, &src.nga_cookie, token).await,
        Platform::LinuxDo => crate::platform::linuxdo::detail(&http, &src.linuxdo_cookie, token).await,
    };
    match r { Ok(v) => Update::Details(v), Err(e) => Update::Error(e.to_string()) }
}
```

> `download_images` 签名改为接受 `&HttpClient`(原接受 `&ZhihuClient`)。改其首参为 `client: &crate::net::HttpClient`,内部 `client.download_image(url)` → `client.fetch_bytes(url)`。

- [ ] **Step 4: 占位平台模块以便编译**

为尚未实现的平台建空壳(后续 Phase 实现),先建最小可编译版本(返回未实现错误),保证本任务可编译:

`src/platform/mod.rs` 增加:

```rust
pub mod v2ex;
pub mod hupu;
pub mod nga;
pub mod linuxdo;
```

`src/platform/v2ex/mod.rs`(以及 hupu 同形):

```rust
use anyhow::Result;
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView};

pub async fn list(_http: &HttpClient) -> Result<Vec<ListEntry>> { anyhow::bail!("未实现") }
pub async fn detail(_http: &HttpClient, _token: &str) -> Result<Vec<DetailView>> { anyhow::bail!("未实现") }
```

`src/platform/nga/mod.rs`:

```rust
use anyhow::Result;
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView};

pub async fn list(_http: &HttpClient, _cookie: &str, _page: u32) -> Result<Vec<ListEntry>> { anyhow::bail!("未实现") }
pub async fn detail(_http: &HttpClient, _cookie: &str, _token: &str) -> Result<Vec<DetailView>> { anyhow::bail!("未实现") }
```

`src/platform/linuxdo/mod.rs`:

```rust
use anyhow::Result;
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView};

pub async fn list(_http: &HttpClient, _cookie: &str) -> Result<Vec<ListEntry>> { anyhow::bail!("未实现") }
pub async fn detail(_http: &HttpClient, _cookie: &str, _token: &str) -> Result<Vec<DetailView>> { anyhow::bail!("未实现") }
```

- [ ] **Step 5: 修复 runner 中其余调用点(handle_key/open_selection/dispatch/apply_update)**

- `apply_update` 的 `Update::Connected { cookie, list }` → `{ platform, cookie, list }`;在该分支调用 `app.switch_platform(platform)` 后 `apply_list_deduped`;持久化时按平台写 cookie(见 Task 2.5,先临时仅知乎写 `cfg.zhihu.cookie`,Phase 5 完善)。
- `open_selection`:把 `(detail, question_id)` 改 `(detail, open_token)`;`(None, Some(token))` 分支发 `Request::Detail { platform: app.active_platform, token }`。
- `Request::Comments`/Tab 分支不变(仅知乎用)。
- `refresh`:按 `app.active_platform` 发 `Request::List(p)`(Recommend→Zhihu 的 More 见下)。改为:刷新发 `Request::List(active_platform)`(重置游标取首页);另加翻页见 Task 2.6。
- `Request::Connect(cookie)` 旧调用(`run_app` 初始化、Login Enter)→ `Request::Connect { platform: Platform::Zhihu, cookie }`。

- [ ] **Step 6: 更新 runner 测试**

测试里所有 `Request::Recommend`/`Request::Answers(..)`/`Request::Connect(..)` 断言改为新变体:
- `Request::Recommend` → `Request::List(Platform::Zhihu)`。
- `Request::Answers(id)` → `Request::Detail { platform: Platform::Zhihu, token }`(`open_selection_on_list_sends_answers` 据此改)。
- `dispatch_zhihu_*` 期望 `Request::List(Platform::Zhihu)`。
- `Update::List` 测试不变。
- `entry()` helper 的 `question_id` 已在 2.1 改为 `open_token`。

- [ ] **Step 7: 编译 + 测试**

Run: `cargo test --lib`
Expected: 全绿(占位平台未被测试触达)。

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(worker): platform-generic Request/Update + Sources dispatch"
```

### Task 2.5: 配置增加 nga/linuxdo cookie

**Files:** Modify `src/config/mod.rs`(现 config 文件)

- [ ] **Step 1: 写失败测试(config tests)**

```rust
    #[test]
    fn config_roundtrips_all_cookies() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.zhihu.cookie = "z".into();
        cfg.nga.cookie = "n".into();
        cfg.linuxdo.cookie = "l".into();
        cfg.save_to(&path).unwrap();
        assert_eq!(cfg, Config::load_from(&path).unwrap());
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib config_roundtrips_all_cookies`
Expected: 编译失败(`nga`/`linuxdo` 字段不存在)。

- [ ] **Step 3: 增加配置结构**

```rust
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub zhihu: ZhihuConfig,
    #[serde(default)]
    pub nga: NgaConfig,
    #[serde(default)]
    pub linuxdo: LinuxDoConfig,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct NgaConfig { #[serde(default)] pub cookie: String }

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct LinuxDoConfig { #[serde(default)] pub cookie: String }
```

- [ ] **Step 4: 提供按平台读写 cookie 的 helper**

```rust
impl Config {
    pub fn cookie_for(&self, p: crate::platform::Platform) -> String {
        use crate::platform::Platform::*;
        match p {
            Zhihu => self.zhihu.cookie.clone(),
            Nga => self.nga.cookie.clone(),
            LinuxDo => self.linuxdo.cookie.clone(),
            _ => String::new(),
        }
    }
    pub fn set_cookie_for(&mut self, p: crate::platform::Platform, cookie: String) {
        use crate::platform::Platform::*;
        match p {
            Zhihu => self.zhihu.cookie = cookie,
            Nga => self.nga.cookie = cookie,
            LinuxDo => self.linuxdo.cookie = cookie,
            _ => {}
        }
    }
}
```

- [ ] **Step 5: 测试 + Commit**

Run: `cargo test --lib config_roundtrips_all_cookies`
Expected: PASS。

```bash
git add -A
git commit -m "feat(config): per-platform cookies (nga, linuxdo) + helpers"
```

### Task 2.6: 命令 + 平台切换 + 登录路由

**Files:** Modify `src/app/command.rs`, `src/app/runner.rs`

- [ ] **Step 1: 在 `command.rs` 增加命令变体与解析**

`Command` 枚举增加:

```rust
    V2ex,
    Hupu,
    Nga,
    LinuxDo,
```

`parse` 增加分支(与现有 `/zhihu` 同形):

```rust
        "/v2ex" => Command::V2ex,
        "/hupu" => Command::Hupu,
        "/nga" => Command::Nga,
        "/linuxdo" => Command::LinuxDo,
```

- [ ] **Step 2: 写失败测试(runner tests)**

```rust
    #[test]
    fn dispatch_v2ex_switches_platform_and_lists() {
        let mut app = App::new();
        let (tx, mut rx) = make_channel();
        dispatch_command(&mut app, Command::V2ex, &tx);
        assert_eq!(app.active_platform, Platform::V2ex);
        match rx.try_recv() {
            Ok(Request::List(Platform::V2ex)) => {}
            other => panic!("expected List(V2ex), got {:?}", other),
        }
    }

    #[test]
    fn dispatch_nga_without_cookie_routes_to_login() {
        let mut app = App::new();
        // no nga cookie configured in default config
        let (tx, mut rx) = make_channel();
        dispatch_command(&mut app, Command::Nga, &tx);
        assert_eq!(app.screen(), &Screen::Login);
        assert_eq!(app.pending_login_platform, Some(Platform::Nga));
        assert!(rx.try_recv().is_err(), "should not fetch before login");
    }
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test --lib dispatch_v2ex_switches_platform_and_lists dispatch_nga_without_cookie_routes_to_login`
Expected: 编译失败。

- [ ] **Step 4: 实现 dispatch 分支 + 通用 helper**

在 `dispatch_command` 增加一个内部 helper 并接线:

```rust
fn open_platform(app: &mut App, p: Platform, req_tx: &mpsc::UnboundedSender<Request>) {
    app.switch_platform(p);
    if p.needs_cookie() {
        let cookie = crate::config::Config::load().unwrap_or_default().cookie_for(p);
        if cookie.is_empty() {
            app.pending_login_platform = Some(p);
            app.error = None;
            app.replace(Screen::Login);
            return;
        }
        app.loading = true;
        let _ = req_tx.send(Request::Connect { platform: p, cookie });
    } else {
        app.loading = true;
        let _ = req_tx.send(Request::List(p));
    }
}
```

`dispatch_command` 中:

```rust
        Command::Zhihu => open_platform(app, Platform::Zhihu, req_tx),
        Command::V2ex => open_platform(app, Platform::V2ex, req_tx),
        Command::Hupu => open_platform(app, Platform::Hupu, req_tx),
        Command::Nga => open_platform(app, Platform::Nga, req_tx),
        Command::LinuxDo => open_platform(app, Platform::LinuxDo, req_tx),
```

> `Command::Zhihu` 原逻辑(空 cookie → Login)被 `open_platform` 统一覆盖(知乎 `needs_cookie()==true`),行为一致(但需把 `pending_login_platform` 设为 Zhihu)。`Command::Hot`/`Command::Search` 保持原样(仅知乎)。

`Login` 屏的 Enter(`handle_key`)改为按 `pending_login_platform`(缺省 Zhihu)连接:

```rust
        KeyCode::Enter => {
            if *app.screen() == Screen::Login {
                let cookie = std::mem::take(&mut app.command);
                if !cookie.is_empty() {
                    let p = app.pending_login_platform.take().unwrap_or(Platform::Zhihu);
                    app.switch_platform(p);
                    app.loading = true;
                    let _ = req_tx.send(Request::Connect { platform: p, cookie });
                }
            } else if !app.command.is_empty() {
                ...
```

`apply_update` 的 `Update::Connected { platform, cookie, list }` 持久化:

```rust
        Update::Connected { platform, cookie, list } => {
            let mut cfg = crate::config::Config::load().unwrap_or_default();
            cfg.set_cookie_for(platform, cookie.clone());
            let _ = cfg.save();
            if platform == Platform::Zhihu { app.cookie = cookie; }
            app.switch_platform(platform);
            app.error = None;
            app.apply_list_deduped(list);
            match app.screen() {
                Screen::List => {}
                Screen::Login => app.replace(Screen::List),
                _ => app.push(Screen::List),
            }
        }
```

`refresh` 改为按平台首页:

```rust
fn refresh(app: &mut App, req_tx: &mpsc::UnboundedSender<Request>) {
    app.loading = true;
    match app.active_platform {
        Platform::Zhihu if app.list_source == ListSource::Hot => { let _ = req_tx.send(Request::HotList); }
        Platform::Zhihu if matches!(app.list_source, ListSource::Search(_)) => {
            if let ListSource::Search(q) = app.list_source.clone() { let _ = req_tx.send(Request::Search(q)); }
        }
        p => { let _ = req_tx.send(Request::List(p)); }
    }
}
```

> 注:知乎推荐的「r 刷新」现在发 `Request::List(Zhihu)`,会**重置游标取首页**。若希望 r 是「下一页」,改发 `Request::More(Zhihu)`。本计划:`r` = 下一页(更符合「翻页看新内容」),即 Zhihu+Recommend 时发 `Request::More(Platform::Zhihu)`,其余发 `List`。最终 `refresh` Zhihu-recommend 分支用 `Request::More(Platform::Zhihu)`。

- [ ] **Step 5: 测试**

Run: `cargo test --lib`
Expected: 全绿。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: /v2ex /hupu /nga /linuxdo commands + login routing + per-platform refresh"
```

---

## Phase 3 — V2EX

### Task 3.1: V2EX 列表与详情解析

**Files:** Modify `src/platform/v2ex/mod.rs`; Create `tests/fixtures/v2ex_list.html`, `tests/fixtures/v2ex_topic.html`

- [ ] **Step 1: 保存夹具**

抓取真实页面存为夹具(执行时运行):

```bash
curl -sL "https://www.v2ex.com/?tab=all" -A "Mozilla/5.0" -o tests/fixtures/v2ex_list.html
# 取列表里任一 /t/<id> 帖子页:
curl -sL "https://www.v2ex.com/t/<某id>" -A "Mozilla/5.0" -o tests/fixtures/v2ex_topic.html
```

- [ ] **Step 2: 写失败测试(纯解析函数,不联网)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v2ex_list() {
        let html = include_str!("../../../tests/fixtures/v2ex_list.html");
        let rows = parse_list(html);
        assert!(!rows.is_empty(), "should parse topic rows");
        let r = &rows[0];
        assert!(!r.title.is_empty());
        assert!(r.open_token.as_deref().unwrap().starts_with("/t/"), "token is /t/<id>");
        assert!(r.detail.is_none(), "v2ex list rows fetch detail on open");
    }

    #[test]
    fn parses_v2ex_detail() {
        let html = include_str!("../../../tests/fixtures/v2ex_topic.html");
        let dv = parse_detail(html);
        assert!(!dv.body.is_empty(), "topic body extracted");
    }
}
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test --lib v2ex`
Expected: 编译失败(`parse_list`/`parse_detail` 未定义)。

- [ ] **Step 4: 实现解析 + 联网封装**

```rust
use anyhow::Result;
use scraper::{Html, Selector};
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView, html};

const BASE: &str = "https://www.v2ex.com";

/// Parse the V2EX index into list rows. Selector: `#Main .box` first block's
/// `.cell.item .topic-link` (title + /t/<id> href, hash/query stripped).
fn parse_list(html_str: &str) -> Vec<ListEntry> {
    let doc = Html::parse_document(html_str);
    let link_sel = Selector::parse("#Main .box .cell.item .topic-link").unwrap();
    let mut rows = Vec::new();
    for a in doc.select(&link_sel) {
        let title = a.text().collect::<String>().trim().to_string();
        let href = a.value().attr("href").unwrap_or("").trim();
        let token = href.split(['#', '?']).next().unwrap_or("").to_string();
        if title.is_empty() || token.is_empty() { continue; }
        rows.push(ListEntry { title, subtitle: String::new(), open_token: Some(token), detail: None });
    }
    rows
}

/// Parse a topic page into a single DetailView. Body = `#Main` text (title h1 removed).
fn parse_detail(html_str: &str) -> DetailView {
    let doc = Html::parse_document(html_str);
    let main_sel = Selector::parse("#Main").unwrap();
    let inner = doc.select(&main_sel).next().map(|m| m.inner_html()).unwrap_or_default();
    let (body, images) = html::to_text_and_images(&inner);
    DetailView { author: String::new(), voteup: 0, body, images, answer_id: String::new() }
}

pub async fn list(http: &HttpClient) -> Result<Vec<ListEntry>> {
    let html_str = http.get_text(&format!("{BASE}/?tab=all"), &[]).await?;
    Ok(parse_list(&html_str))
}

pub async fn detail(http: &HttpClient, token: &str) -> Result<Vec<DetailView>> {
    let url = if token.starts_with("http") { token.to_string() } else { format!("{BASE}{token}") };
    let html_str = http.get_text(&url, &[]).await?;
    let mut dv = parse_detail(&html_str);
    dv.answer_id = token.to_string(); // image-cache owner key
    Ok(vec![dv])
}
```

- [ ] **Step 5: 运行测试**

Run: `cargo test --lib v2ex`
Expected: PASS(若选择器与真实页面不符,据 fixture 调整选择器后再过)。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(v2ex): list + topic detail parsing"
```

---

## Phase 4 — 虎扑

### Task 4.1: 虎扑列表与详情解析

**Files:** Modify `src/platform/hupu/mod.rs`; Create `tests/fixtures/hupu_list.html`, `tests/fixtures/hupu_topic.html`

- [ ] **Step 1: 保存夹具**

```bash
curl -sL "https://bbs.hupu.com/all-gambia" -A "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/80.0" -o tests/fixtures/hupu_list.html
curl -sL "https://bbs.hupu.com/<某帖>.html" -A "Mozilla/5.0 ... Chrome/80.0" -o tests/fixtures/hupu_topic.html
```

- [ ] **Step 2: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hupu_list() {
        let html = include_str!("../../../tests/fixtures/hupu_list.html");
        let rows = parse_list(html);
        assert!(!rows.is_empty());
        assert!(!rows[0].title.is_empty());
        assert!(rows[0].open_token.is_some());
    }

    #[test]
    fn parses_hupu_detail_strips_css_mangle() {
        let html = include_str!("../../../tests/fixtures/hupu_topic.html");
        let dv = parse_detail(html);
        assert!(!dv.body.is_empty());
    }
}
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test --lib hupu`
Expected: 编译失败。

- [ ] **Step 4: 实现**

```rust
use anyhow::Result;
use scraper::{Html, Selector};
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView, html};

const BASE: &str = "https://bbs.hupu.com";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/80.0.3987.132 Safari/537.36";

/// `.text-list-model .list-item a .t-title` → title; the `<a href>` → token.
fn parse_list(html_str: &str) -> Vec<ListEntry> {
    let doc = Html::parse_document(html_str);
    let item_sel = Selector::parse(".text-list-model .list-item a").unwrap();
    let title_sel = Selector::parse(".t-title").unwrap();
    let mut rows = Vec::new();
    for a in doc.select(&item_sel) {
        let title = a.select(&title_sel).next()
            .map(|t| t.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let mut href = a.value().attr("href").unwrap_or("").trim().to_string();
        if !href.is_empty() && !href.starts_with("http") && !href.starts_with('/') {
            href = format!("/{href}");
        }
        if title.is_empty() || href.is_empty() { continue; }
        rows.push(ListEntry { title, subtitle: String::new(), open_token: Some(href), detail: None });
    }
    rows
}

/// Remove hupu's CSS-obfuscation strings (`__xxx"` / `__xxx ` patterns) then take
/// `.index_bbs-post-web-body-left-wrapper`.
fn parse_detail(html_str: &str) -> DetailView {
    let cleaned = strip_css_mangle(html_str);
    let doc = Html::parse_document(&cleaned);
    let sel = Selector::parse(".index_bbs-post-web-body-left-wrapper").unwrap();
    let inner = doc.select(&sel).next().map(|m| m.inner_html()).unwrap_or_default();
    let (body, images) = html::to_text_and_images(&inner);
    DetailView { author: String::new(), voteup: 0, body, images, answer_id: String::new() }
}

fn strip_css_mangle(s: &str) -> String {
    // Replace `__<word>"` -> `"` and `__<word> ` -> ` ` (mirrors the reference regexes).
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'_' && i + 1 < bytes.len() && bytes[i + 1] == b'_' {
            let mut j = i + 2;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') { j += 1; }
            if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b' ') {
                out.push(bytes[j] as char);
                i = j + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

pub async fn list(http: &HttpClient) -> Result<Vec<ListEntry>> {
    let html_str = http.get_text(&format!("{BASE}/all-gambia"), &[("user-agent", UA)]).await?;
    Ok(parse_list(&html_str))
}

pub async fn detail(http: &HttpClient, token: &str) -> Result<Vec<DetailView>> {
    let mut url = if token.starts_with("http") { token.to_string() } else { format!("{BASE}{token}") };
    url = url.trim_end_matches(".html").to_string() + ".html"; // normalize
    let html_str = http.get_text(&url, &[("user-agent", UA)]).await?;
    let mut dv = parse_detail(&html_str);
    dv.answer_id = token.to_string();
    Ok(vec![dv])
}
```

> `strip_css_mangle` 按字节处理仅对 ASCII 安全替换;中文为多字节但不含 `_`/`"` 字节模式,逐字节 push 会破坏 UTF-8。**改为按 `char` 迭代**实现以保证 UTF-8 安全:

```rust
fn strip_css_mangle(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' && chars.peek() == Some(&'_') {
            chars.next(); // consume 2nd '_'
            // skip word chars
            while matches!(chars.peek(), Some(ch) if ch.is_ascii_alphanumeric() || *ch == '_') {
                chars.next();
            }
            // keep the delimiter (" or space) if present
            match chars.peek() {
                Some('"') => { out.push('"'); chars.next(); }
                Some(' ') => { out.push(' '); chars.next(); }
                _ => {}
            }
            continue;
        }
        out.push(c);
    }
    out
}
```

(采用上面的 char 版本,删除前面的字节版本。)

- [ ] **Step 5: 测试**

Run: `cargo test --lib hupu`
Expected: PASS(据 fixture 微调选择器)。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(hupu): list + detail parsing (utf8-safe css de-mangle)"
```

---

## Phase 5 — NGA

### Task 5.1: GBK 解码 helper

**Files:** Modify `src/platform/nga/mod.rs`

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_gbk_bytes() {
        // GBK bytes for "中文": 0xD6D0 0xCEC4
        let bytes = [0xD6u8, 0xD0, 0xCE, 0xC4];
        assert_eq!(gbk_to_string(&bytes), "中文");
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib nga::tests::decodes_gbk_bytes`
Expected: 编译失败。

- [ ] **Step 3: 实现**

```rust
use anyhow::Result;
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView};

const BASE: &str = "https://bbs.nga.cn";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
pub const DEFAULT_FID: &str = "-7"; // 网事杂谈

fn gbk_to_string(bytes: &[u8]) -> String {
    let (cow, _, _) = encoding_rs::GBK.decode(bytes);
    cow.into_owned()
}
```

- [ ] **Step 4: 测试 + Commit**

Run: `cargo test --lib nga::tests::decodes_gbk_bytes`
Expected: PASS。

```bash
git add -A && git commit -m "feat(nga): GBK decode helper"
```

### Task 5.2: NGA 列表(thread.php lite=xml)

**Files:** Modify `src/platform/nga/mod.rs`; Create `tests/fixtures/nga_list.xml`(UTF-8 转存)

- [ ] **Step 1: 保存夹具(转成 UTF-8 便于 include_str!)**

```bash
curl -sL "https://bbs.nga.cn/thread.php?fid=-7&page=1&lite=xml" -A "Mozilla/5.0 ... Chrome/124.0" -H "Cookie: <你的nga cookie>" --output - | iconv -f GBK -t UTF-8 > tests/fixtures/nga_list.xml
```

- [ ] **Step 2: 写失败测试**

```rust
    #[test]
    fn parses_nga_list() {
        let xml = include_str!("../../../tests/fixtures/nga_list.xml");
        let rows = parse_list(xml);
        assert!(!rows.is_empty());
        let r = &rows[0];
        assert!(r.title.starts_with('['), "title shows [replies] prefix");
        assert!(r.open_token.as_deref().unwrap().contains("tid="));
    }
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test --lib nga::tests::parses_nga_list`
Expected: 编译失败。

- [ ] **Step 4: 实现 `parse_list`(quick-xml 事件流)**

```rust
use quick_xml::events::Event;
use quick_xml::Reader;

/// Parse thread.php?lite=xml. Items live under <__T>/<item> with <tid>,<subject>,<replies>.
fn parse_list(xml: &str) -> Vec<ListEntry> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut rows = Vec::new();
    let mut in_t = false;          // inside __T
    let mut in_item = false;
    let mut cur_tag = String::new();
    let (mut tid, mut subject, mut replies) = (String::new(), String::new(), String::new());
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "__T" => in_t = true,
                    "item" if in_t => { in_item = true; tid.clear(); subject.clear(); replies.clear(); }
                    _ => cur_tag = name,
                }
            }
            Ok(Event::Text(t)) if in_item => {
                let txt = t.unescape().unwrap_or_default().to_string();
                match cur_tag.as_str() {
                    "tid" => tid.push_str(&txt),
                    "subject" => subject.push_str(&txt),
                    "replies" => replies.push_str(&txt),
                    _ => {}
                }
            }
            Ok(Event::CData(t)) if in_item => {
                let txt = String::from_utf8_lossy(t.as_ref()).to_string();
                match cur_tag.as_str() {
                    "subject" => subject.push_str(&txt),
                    "tid" => tid.push_str(&txt),
                    "replies" => replies.push_str(&txt),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "item" if in_item => {
                        if !tid.is_empty() && !subject.is_empty() {
                            let r = if replies.is_empty() { "0".into() } else { replies.clone() };
                            rows.push(ListEntry {
                                title: format!("[{r}] {subject}"),
                                subtitle: String::new(),
                                open_token: Some(format!("/read.php?tid={tid}")),
                                detail: None,
                            });
                        }
                        in_item = false;
                    }
                    "__T" => in_t = false,
                    _ => {}
                }
                cur_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    rows
}

pub async fn list(http: &HttpClient, cookie: &str, page: u32) -> Result<Vec<ListEntry>> {
    if cookie.is_empty() {
        return Ok(vec![ListEntry {
            title: "NGA 未配置 cookie(回车去配置)".into(),
            subtitle: String::new(), open_token: None, detail: None,
        }]);
    }
    let url = format!("{BASE}/thread.php?fid={DEFAULT_FID}&page={page}&lite=xml");
    let bytes = http.get_bytes(&url, &[("cookie", cookie), ("user-agent", UA)]).await?;
    Ok(parse_list(&gbk_to_string(&bytes)))
}
```

- [ ] **Step 5: 测试**

Run: `cargo test --lib nga::tests::parses_nga_list`
Expected: PASS(据 fixture 调整标签名;若 NGA 用属性而非子元素,改读 `Event::Empty`/属性)。

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(nga): thread list (GBK xml via quick-xml)"
```

### Task 5.3: NGA 详情(read.php lite=xml)+ BBCode 清洗

**Files:** Modify `src/platform/nga/mod.rs`; Create `tests/fixtures/nga_topic.xml`

- [ ] **Step 1: 保存夹具**

```bash
curl -sL "https://bbs.nga.cn/read.php?tid=<某tid>&lite=xml" -A "...Chrome/124.0" -H "Cookie: <nga cookie>" | iconv -f GBK -t UTF-8 > tests/fixtures/nga_topic.xml
```

- [ ] **Step 2: 写失败测试**

```rust
    #[test]
    fn parses_nga_detail_concats_floors() {
        let xml = include_str!("../../../tests/fixtures/nga_topic.xml");
        let dvs = parse_detail(xml);
        assert_eq!(dvs.len(), 1, "thread renders as a single detail");
        assert!(!dvs[0].body.is_empty());
    }

    #[test]
    fn cleans_nga_bbcode() {
        let raw = "看[b]这里[/b][quote]引用[/quote][s:ac:茶]结束";
        let out = clean_bbcode(raw);
        assert!(!out.contains("[b]") && !out.contains("[/quote]"));
        assert!(out.contains("看") && out.contains("结束"));
    }
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test --lib nga::tests::cleans_nga_bbcode nga::tests::parses_nga_detail_concats_floors`
Expected: 编译失败。

- [ ] **Step 4: 实现 `clean_bbcode` + `parse_detail`**

```rust
/// Strip NGA BBCode/smileys to readable text. Images `[img]./xxx[/img]` become the
/// real CDN URL and are collected (rendered as 【图N】 by the caller's flow); other
/// tags `[x]..[/x]` and smileys `[s:..:..]` are removed.
fn clean_bbcode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            // consume until matching ']'
            let mut tag = String::new();
            while let Some(&n) = chars.peek() {
                chars.next();
                if n == ']' { break; }
                tag.push(n);
            }
            // drop the tag entirely (img handled separately in parse_detail via regex-free scan)
            let _ = tag;
            continue;
        }
        out.push(c);
    }
    out
}

/// Extract NGA attachment image URLs from raw content (`[img]./xxx[/img]` and
/// `[img]/xxx[/img]`). Returns absolute CDN urls.
fn extract_nga_images(s: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("[img]") {
        let after = &rest[start + 5..];
        if let Some(end) = after.find("[/img]") {
            let raw = after[..end].trim();
            let path = raw.trim_start_matches("./");
            let url = if path.starts_with("http") {
                path.to_string()
            } else {
                format!("https://img.nga.178.com/attachments/{path}")
            };
            urls.push(url);
            rest = &after[end + 6..];
        } else { break; }
    }
    urls
}

/// Parse read.php?lite=xml: main post + each reply concatenated into one body.
/// Floors live under <__R>/<item> with <content> (and author under <__U> mapping,
/// simplified here to floor index). Adjust tag names to fixture.
fn parse_detail(xml: &str) -> Vec<DetailView> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_item = false;
    let mut cur_tag = String::new();
    let mut content = String::new();
    let mut floors: Vec<String> = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" { in_item = true; content.clear(); }
                cur_tag = name;
            }
            Ok(Event::Text(t)) | Ok(Event::CData(t)) if in_item && cur_tag == "content" => {
                // Text variant needs unescape; CData is raw bytes. Handle both.
                let txt = match std::str::from_utf8(t.as_ref()) {
                    Ok(s) => s.to_string(),
                    Err(_) => String::new(),
                };
                content.push_str(&txt);
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" && in_item {
                    if !content.trim().is_empty() { floors.push(content.clone()); }
                    in_item = false;
                }
                cur_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    let mut images = Vec::new();
    let mut body = String::new();
    for (i, raw) in floors.iter().enumerate() {
        for u in extract_nga_images(raw) {
            images.push(u);
            // marker appended after text for that floor
        }
        let text = clean_bbcode(raw);
        if i == 0 {
            body.push_str(&text);
        } else {
            body.push_str(&format!("\n\n── #{i} ──\n{text}", i = i));
        }
    }
    // append image markers at end so digit keys still map 1..N
    for n in 1..=images.len() {
        body.push_str(&format!("\n【图{n}】"));
    }
    vec![DetailView { author: String::new(), voteup: floors.len() as i64, body: body.trim().to_string(), images, answer_id: String::new() }]
}

pub async fn detail(http: &HttpClient, cookie: &str, token: &str) -> Result<Vec<DetailView>> {
    let tid = token.split("tid=").nth(1).map(|s| s.split('&').next().unwrap_or(s)).unwrap_or("");
    if tid.is_empty() { anyhow::bail!("无法解析 NGA tid"); }
    let url = format!("{BASE}/read.php?tid={tid}&lite=xml");
    let bytes = http.get_bytes(&url, &[("cookie", cookie), ("user-agent", UA)]).await?;
    let mut dvs = parse_detail(&gbk_to_string(&bytes));
    if let Some(d) = dvs.first_mut() { d.answer_id = tid.to_string(); }
    Ok(dvs)
}
```

> CData 与 Text 混合分支写法注意:`Ok(Event::Text(t)) | Ok(Event::CData(t))` 两者 `t` 类型不同(`BytesText` vs `BytesCData`),不能合并模式。**拆成两个分支**,Text 用 `t.unescape()`,CData 用 `String::from_utf8_lossy(t.as_ref())`。执行时按此拆分。

- [ ] **Step 5: 测试**

Run: `cargo test --lib nga`
Expected: PASS(据真实 fixture 调整标签:NGA `read.php` 的楼层容器/内容标签可能是 `__R`/`content`/`subject`;以 fixture 为准)。

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(nga): thread detail (floors concat, bbcode/img clean)"
```

---

## Phase 6 — Linux.do

### Task 6.1: RSS 列表与详情解析

**Files:** Modify `src/platform/linuxdo/mod.rs`; Create `tests/fixtures/linuxdo_latest.rss`, `tests/fixtures/linuxdo_topic.rss`

- [ ] **Step 1: 保存夹具**

```bash
curl -sL "https://linux.do/latest.rss" -A "Mozilla/5.0 ... Chrome/142.0" -H "Cookie: <linuxdo cookie>" -o tests/fixtures/linuxdo_latest.rss
curl -sL "https://linux.do/t/topic/<id>.rss" -A "..." -H "Cookie: <linuxdo cookie>" -o tests/fixtures/linuxdo_topic.rss
```

- [ ] **Step 2: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linuxdo_list() {
        let rss = include_str!("../../../tests/fixtures/linuxdo_latest.rss");
        let rows = parse_list(rss);
        assert!(!rows.is_empty());
        assert!(rows[0].open_token.as_deref().unwrap().starts_with("http"));
    }

    #[test]
    fn parses_linuxdo_detail_main_plus_replies() {
        let rss = include_str!("../../../tests/fixtures/linuxdo_topic.rss");
        let dvs = parse_detail(rss);
        assert_eq!(dvs.len(), 1);
        assert!(!dvs[0].body.is_empty());
    }
}
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test --lib linuxdo`
Expected: 编译失败。

- [ ] **Step 4: 实现(quick-xml 解析 RSS item)**

```rust
use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView, html};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";

struct RssItem { title: String, link: String, description: String }

fn parse_items(rss: &str) -> Vec<RssItem> {
    let mut reader = Reader::from_str(rss);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut items = Vec::new();
    let mut in_item = false;
    let mut cur = String::new();
    let (mut title, mut link, mut desc) = (String::new(), String::new(), String::new());
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" { in_item = true; title.clear(); link.clear(); desc.clear(); }
                cur = name;
            }
            Ok(Event::Text(t)) if in_item => {
                let s = t.unescape().unwrap_or_default().to_string();
                match cur.as_str() { "title" => title.push_str(&s), "link" => link.push_str(&s), "description" => desc.push_str(&s), _ => {} }
            }
            Ok(Event::CData(t)) if in_item => {
                let s = String::from_utf8_lossy(t.as_ref()).to_string();
                match cur.as_str() { "title" => title.push_str(&s), "link" => link.push_str(&s), "description" => desc.push_str(&s), _ => {} }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" && in_item {
                    items.push(RssItem { title: title.clone(), link: link.clone(), description: desc.clone() });
                    in_item = false;
                }
                cur.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    items
}

fn parse_list(rss: &str) -> Vec<ListEntry> {
    parse_items(rss).into_iter().filter_map(|it| {
        if it.title.is_empty() || it.link.is_empty() { return None; }
        Some(ListEntry { title: it.title, subtitle: String::new(), open_token: Some(it.link), detail: None })
    }).collect()
}

/// Topic RSS is reverse-chronological: last item is the original post (主楼). Concat
/// main post + replies (skip the "阅读完整话题" trailing link).
fn parse_detail(rss: &str) -> Vec<DetailView> {
    let items = parse_items(rss);
    if items.is_empty() {
        return vec![DetailView { author: String::new(), voteup: 0, body: String::new(), images: vec![], answer_id: String::new() }];
    }
    let clean = |d: &str| d
        .replace("<p><a href=\"\">阅读完整话题</a></p>", "");
    let mut body = String::new();
    let mut images = Vec::new();
    // main post = last item
    let main = &items[items.len() - 1];
    let (mt, mi) = html::to_text_and_images(&clean(&main.description));
    body.push_str(&mt);
    images.extend(mi);
    // replies = items[0..len-1] in chronological order (reverse of RSS)
    for (idx, it) in items[..items.len() - 1].iter().rev().enumerate() {
        let (t, im) = html::to_text_and_images(&clean(&it.description));
        body.push_str(&format!("\n\n── #{} ──\n{}", idx + 1, t));
        images.extend(im);
    }
    vec![DetailView { author: String::new(), voteup: (items.len() as i64 - 1).max(0), body: body.trim().to_string(), images, answer_id: String::new() }]
}

fn headers(cookie: &str) -> Vec<(&'static str, String)> {
    vec![("user-agent", UA.to_string()), ("cookie", cookie.to_string()),
         ("accept", "application/xml,text/html;q=0.9,*/*;q=0.8".to_string())]
}

pub async fn list(http: &HttpClient, cookie: &str) -> Result<Vec<ListEntry>> {
    if cookie.is_empty() {
        return Ok(vec![ListEntry { title: "Linux.do 未配置 cookie(回车去配置)".into(),
            subtitle: String::new(), open_token: None, detail: None }]);
    }
    let h = headers(cookie);
    let hr: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let rss = http.get_text("https://linux.do/latest.rss", &hr).await?;
    Ok(parse_list(&rss))
}

pub async fn detail(http: &HttpClient, cookie: &str, token: &str) -> Result<Vec<DetailView>> {
    let url = if token.ends_with(".rss") { token.to_string() } else { format!("{}.rss", token.trim_end_matches('/')) };
    let h = headers(cookie);
    let hr: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let rss = http.get_text(&url, &hr).await?;
    let mut dvs = parse_detail(&rss);
    if let Some(d) = dvs.first_mut() { d.answer_id = token.to_string(); }
    Ok(dvs)
}
```

- [ ] **Step 5: 测试**

Run: `cargo test --lib linuxdo`
Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(linuxdo): RSS list + topic detail (main+replies)"
```

---

## Phase 7 — UI 平台标识 + 收尾

### Task 7.1: 状态栏显示活跃平台

**Files:** Modify `src/ui/screens.rs`

- [ ] **Step 1: 找到非伪装状态行渲染处**

定位状态栏/标题渲染(列表页 footer 或 header)。在非伪装(`!app.camouflage`)或 help 区域,加入 `app.active_platform.label()`。伪装开启时不显平台名(保持 Claude Code 伪装)。

- [ ] **Step 2: 加入 label**

在状态行字符串拼接当前平台,例如帮助/提示行:

```rust
let plat = app.active_platform.label();
// 拼到现有非伪装状态文案中,如:format!("{plat} · /help 查看命令")
```

- [ ] **Step 3: 手动验证编译**

Run: `cargo build`
Expected: 通过。

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(ui): show active platform label in status line (non-camouflaged)"
```

### Task 7.2: 全量校验 + clippy

- [ ] **Step 1: clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 无警告(如有,修复)。

- [ ] **Step 2: 全量测试**

Run: `cargo test --lib`
Expected: 全绿。

- [ ] **Step 3: 实网冒烟(可选,需各平台 cookie)**

Run: `cargo test --lib -- --ignored`
Expected: 各平台 live 测试通过(无 cookie 的自动跳过)。

- [ ] **Step 4: 手动跑 TUI**

Run: `cargo run`
依次 `/v2ex`、`/hupu`、`/nga`、`/linuxdo`、`/zhihu`,验证列表/详情/图片数字键/老板键/伪装开关均正常,知乎 `r` 翻页不再重复。

---

## Phase 8 — 发布

### Task 8.1: 版本号与发布

**Files:** Modify `Cargo.toml`, `npm/package.json`, `README.md`

- [ ] **Step 1: bump 版本到 0.2.0(新增多平台,minor)**

`Cargo.toml` `version = "0.2.0"`;`npm/package.json` `"version": "0.2.0"`。

- [ ] **Step 2: README 增加平台与命令说明**

在使用说明加入 `/v2ex /hupu /nga /linuxdo`,并注明 NGA/Linux.do 需各自 cookie。

- [ ] **Step 3: 刷新 lock + 提交**

```bash
cargo build
git add -A
git commit -m "release: v0.2.0 多平台(V2EX/虎扑/NGA/Linux.do) + 知乎真翻页"
```

- [ ] **Step 4: 推送 + 打 tag**

```bash
git push origin main:master && git push origin main
git tag v0.2.0 && git push origin v0.2.0
```

- [ ] **Step 5: 盯 Actions**

确认 release workflow 4 个 job 全绿(create-release / macOS / Windows / npm),GitHub Release 资产齐全,npm latest = 0.2.0。

---

## 自查(Self-Review)

- **Spec 覆盖**:知乎真翻页(Task 1.1/1.2)✓;Platform 抽象(2.2/2.4)✓;ListEntry 泛化(2.1)✓;全平台去重(2.3)✓;V2EX(3.1)✓;虎扑(4.1)✓;NGA(5.1-5.3)✓;Linux.do(6.1)✓;配置(2.5)✓;登录路由(2.6)✓;命令(2.6)✓;UI 平台名(7.1)✓;论坛主楼+回复拼接(5.3/6.1)✓;新依赖(0.1)✓;共享 html(0.3)✓。
- **类型一致**:`recommend(Option<&str>) -> (Vec<ListEntry>, Option<String>)`、各平台 `list`/`detail` 签名在 Task 2.4 占位与各实现任务一致;`open_token` 全程统一;`Request::List(Platform)` / `Request::Detail{platform,token}` / `Request::Connect{platform,cookie}` 在 dispatch、worker、测试中一致;`apply_list_deduped` / `switch_platform` / `cookie_for` / `set_cookie_for` 命名统一。
- **占位扫描**:夹具内容为执行时 `curl` 真实抓取(非占位);选择器/标签名以 fixture 为准微调(已在对应 Step 标注)。
- **已知执行注意点**:Task 5.3 的 Text/CData 模式需拆分(已注明);Task 4.1 用 char 版 `strip_css_mangle`(已注明);NGA `read.php` 标签名以真实 fixture 为准。
