# touch-fish 设计文档

> 一个在 cmd 里摸鱼的命令行工具。v1 支持知乎,后续可接入百度贴吧。
> 交互方式:斜杠命令(切板块/切账号)+ 方向键(浏览内容)。

日期:2026-06-01

---

## 1. 目标与范围

### 1.1 这是什么

在终端(Windows cmd / PowerShell / macOS / Linux 终端)里,用方向键浏览知乎内容的只读摸鱼工具。界面完全画在当前终端窗口内,不弹新窗口、不开浏览器。

### 1.2 v1 范围

**做:**

1. 热榜 / 最新列表(进入知乎后默认看到)
2. 搜索(`/search 关键词`)
3. 文章 / 回答详情(正文可滚动)
4. 评论(根评论 + 展开子评论)

**不做(放后续版本):** 点赞、关注、发评论、多账号并存、百度贴吧。

### 1.3 非目标

- 不做内容互动(只读)。
- 不做图片渲染(cmd 显示不了,正文里的图显示占位符)。
- v1 不加密 cookie(明文存本地)。

---

## 2. 交互模型

### 2.1 混合交互:斜杠命令 + 方向键

界面底部常驻一条命令行(类似 vim 底部命令栏)。命令负责"跳转"(切板块、切账号、搜索),方向键负责在内容里浏览。命令行**一直显示**。

### 2.2 屏幕与导航

**根屏(启动时):**

```
┌─ touch-fish ─────────────────────────────────┐
│  输入 /zhihu 进入知乎                          │
│  (以后: /tieba 进入贴吧)                       │
├──────────────────────────────────────────────┤
│ > /zhihu_                                      │
└──────────────────────────────────────────────┘
```

**`/zhihu`** → 进入知乎:

- 程序检查本地配置有没有有效 cookie。
- **没有** → 自动进入登录流程(见 2.3)。
- **有** → 直接进入热榜列表。

**知乎内的屏幕:**

- **列表屏(热榜 / 搜索结果)**:`↑↓` 移动光标,`Enter` 进详情,`←`/`Esc` 返回上一级。
- **详情屏**:`↑↓` 滚动正文,`→`/`Tab` 切到评论,`←`/`Esc` 返回列表。
- **评论屏**:`↑↓` 滚动,`Enter` 展开/收起某条根评论的子评论,`←`/`Esc` 返回详情。

### 2.3 命令一览

| 命令 | 可用位置 | 作用 |
|------|---------|------|
| `/zhihu` | 根屏 | 进入知乎(无 cookie 则走登录) |
| `/search 关键词` | 知乎内 | 搜索,结果进列表屏 |
| `/login` | 知乎内 | 重新走登录流程,粘新 cookie **覆盖**旧的(切换账号) |
| `/back` 或 `Esc` | 任意 | 退回上一级 |
| `/quit` 或 `q` | 任意 | 退出 |

### 2.4 登录流程

触发时机:进入 `/zhihu` 时本地无 cookie,或主动输入 `/login`。

流程:

1. 进登录屏,提示用户从浏览器复制知乎 Cookie 请求头(F12 → Network → 任意知乎请求 → 复制 `Cookie`)并粘贴。
2. 程序拿粘贴的 cookie 发一个测试请求(如拉热榜)验证有效性。
3. 有效 → 写入本地配置文件,进入热榜。
4. 无效 → 提示重新粘贴。

`/login` 粘新 cookie 后,旧的直接覆盖(v1 单账号)。

---

## 3. 架构

### 3.1 模块结构

单个 Rust 二进制 crate,模块边界清晰,便于后续加贴吧 / 加纯命令前端。

```
touch-fish/
  Cargo.toml
  assets/
    zhihu.raw.js           # 内嵌的 304KB 签名 JS,include_str! 打进二进制
  src/
    main.rs                # 初始化终端 + 启动事件循环
    app/
      mod.rs               # App 状态机
      state.rs             # Screen 枚举(Root/Login/Search/ZhihuList/ZhihuDetail/ZhihuComments)、导航栈
      command.rs           # 解析斜杠命令
      event.rs             # 按键 -> 动作映射
    ui/
      mod.rs               # 按当前屏幕分发渲染
      root.rs
      list.rs              # 列表(光标 + 滚动视口,处理 CJK 宽度)
      detail.rs            # 详情正文滚动
      comments.rs          # 评论
      command_bar.rs       # 底部命令行
      login.rs             # cookie 粘贴流程
    platform/
      mod.rs               # trait Platform(贴吧以后实现同一 trait)
      zhihu/
        mod.rs             # ZhihuClient: hot_list/search/detail/comments
        api.rs             # 接口 URL 与请求构造
        sign.rs            # x-zse-96 签名(rquickjs 跑内嵌 JS)
        model.rs           # serde 响应结构体
        html.rs            # HTML 正文 -> 终端样式文本
    config/
      mod.rs               # 读写 config.toml,路径解析
    net/
      mod.rs               # reqwest 封装,统一 headers,异步请求
```

### 3.2 技术选型

| 用途 | 库 |
|------|-----|
| 终端界面 | `ratatui` + `crossterm` |
| 异步运行时 | `tokio` |
| HTTP | `reqwest`(rustls) |
| 解析 | `serde` / `serde_json` |
| 签名 x-zse-96 | `rquickjs`(QuickJS)跑内嵌 `zhihu.raw.js` |
| 配置 | `toml` + `dirs` |
| 中文宽度 | `unicode-width` |
| HTML→文本 | `scraper` |
| 错误处理 | `anyhow` / `thiserror` |

### 3.3 异步事件循环

主循环用 `select` 同时盯三件事:

1. 键盘事件(crossterm 异步事件流)
2. 网络结果 channel(tokio mpsc)
3. 定时刷新 tick(加载动画等)

用户触发的网络请求(进详情、搜索等)spawn 到 tokio 任务,界面立刻显示"加载中…"不阻塞,结果通过 channel 回来后重绘。

---

## 4. 知乎数据获取

参考开源项目 `ylw1997/touchFish`(VS Code 扩展,TypeScript)的知乎实现。

### 4.1 接口

| 功能 | 接口 |
|------|------|
| 热榜 | `GET /api/v3/feed/topstory/hot-lists/total?limit=50&desktop=true` |
| 搜索 | `GET /api/v4/search_v3?t=general&q={q}&offset=0&limit=20` |
| 问题/回答详情 | `GET /api/v4/questions/{id}/feeds?include=...&limit=30&offset=0` |
| 根评论 | `GET /api/v4/comment_v5/answers/{id}/root_comment?order_by=score&limit=100` |
| 子评论 | `GET /api/v4/comment_v5/comment/{cid}/child_comment?order_by=ts&limit=20` |

### 4.2 认证

用户自己从浏览器复制知乎 Cookie 请求头(含 `d_c0` 与登录态)。程序把 cookie 带在每个请求上。

### 4.3 签名 x-zse-96(核心)

知乎接口需要 `x-zse-96` 签名头,配合 `x-zse-93: "101_3_3.0"`。

**方案(路线 A,稳):** 内嵌 touchFish 用的 `zhihu.raw.js`(304KB 混淆 JS,导出 `encrypt()` 函数),用 `rquickjs` 在 Rust 里执行它来生成签名。保证与知乎前端一致,实现第一天就能拿到真实数据。

- 启动时建一个 QuickJS 上下文,eval `zhihu.raw.js` 拿到 `encrypt` 函数。
- 每个请求:构造待签字符串(格式为 `"101_3_3.0+{path+query}+{d_c0值}"`,具体格式在实现时从 touchFish 的 `zhihu.ts` 精确抽取),调 `encrypt` 得到 `x-zse-96`。

**后续优化(路线 B):** touchFish 仓库里另有一个纯算法版 `sign.ts`(SHA-1 → 按固定下标取字符 + 20 个魔数 XOR 打散 → base64,约 22 行),可翻译成纯 Rust 去掉 JS 引擎依赖。但其输出格式与真实 `x-zse-96`(`2.0_` 前缀)对不上,**未验证可用**,作为"能省掉 JS 引擎就省"的后续项,需先用真实请求验证。

### 4.4 请求头

```
Cookie: {用户 cookie}
x-zse-96: {签名}
x-zse-93: 101_3_3.0
User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) ...
x-api-version: 3.0.91   (部分接口需要)
```

---

## 5. 配置

- 路径:`dirs::config_dir()/touch-fish/config.toml`(Windows 为 `%APPDATA%`,跨平台)。
- 内容:

```toml
[zhihu]
cookie = "..."
```

- 明文存储(v1 不加密)。
- 文件缺失/损坏 → 创建默认空配置,进入时走登录流程。

---

## 6. 错误处理

| 情况 | 行为 |
|------|------|
| 网络错误 / 非 200 | 底部命令行显示红色错误,不崩溃,停在当前屏 |
| Cookie 失效(401) | 自动跳登录流程,提示重新粘贴 |
| JSON 解析失败(知乎改版) | 显示"解析失败",不崩溃,记录原始响应便于排查 |
| 启动时 JS 引擎初始化失败 | 致命错误,退出并提示 |
| 配置文件缺失/损坏 | 创建默认配置,走登录流程 |

---

## 7. 测试

- **签名黄金测试(最关键):** 用从浏览器 / touchFish 抓到的"已知输入 → 已知 x-zse-96"做断言,验证 rquickjs 跑出来的签名与知乎前端一致。
- 命令解析(`command.rs`):单元测试各命令与参数。
- 配置读写(`config`):单元测试加载/保存/缺失/损坏。
- HTML→文本(`html.rs`):单元测试若干真实回答片段。
- serde 反序列化:用抓存的真实 JSON 样本作 fixture 测各 model。

---

## 8. 后续版本(非 v1)

- 互动:点赞、关注、发评论(touchFish 已有对应接口)。
- 百度贴吧:实现同一个 `Platform` trait。
- 签名路线 B:纯 Rust 重写,去掉 JS 引擎依赖。
- 摸鱼增强:老板键一键隐藏。
- 可能的纯命令前端:复用 `platform` 数据层。
