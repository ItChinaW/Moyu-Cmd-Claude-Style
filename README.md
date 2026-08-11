# touch-fish 🐟

[![npm](https://img.shields.io/npm/v/moyu-fish.svg)](https://www.npmjs.com/package/moyu-fish)

在终端(cmd / PowerShell / macOS / Linux)里摸鱼。支持知乎、V2EX、虎扑、NGA、Linux.do、股票自选和本地电子书,统一「列表 → 详情」交互,全程方向键浏览,界面就画在当前终端窗口里。

## 安装

npm 包地址:https://www.npmjs.com/package/moyu-fish

```bash
# 直接运行(无需安装)
npx moyu-fish

# 或全局安装
npm install -g moyu-fish
moyu
```

安装 npm 包时不会预先下载股票扩展时段抓取依赖；首次进入股票模块并触发美股扩展时段抓取时，程序会优先尝试本地 Python `yfinance` WebSocket 子进程，失败后再回退到现有 Yahoo 方案。

## 本地调试

在仓库中执行：

```bash
cd npm
npm link
moyu-test
```

`moyu-test` 会优先运行当前仓库的 `cargo run`，方便调试本地改动；普通 npm 安装环境则回退到已下载的原生程序。

## 使用

1. 运行:
   ```bash
   cargo run --release --bin moyu
   ```
2. 启动后是平台选择列表,`↑↓` 选平台、回车进入。想免登录先体验,选 **V2EX**、**虎扑** 或 **股票**(无需 cookie)。
   知乎 / NGA / Linux.do 需要 cookie,首次进入会进入登录流程,粘贴对应站点的 Cookie:
   登录该站点 → 按 **F12** → **Network** 标签 → 刷新 → 点任意一个本站请求 → 在 **Request Headers** 里复制 `cookie:` 整行的值 → 粘贴到命令行回车。
   程序会发一个测试请求验证 Cookie,通过后保存到本地配置(各平台独立),之后启动直接可用。

## 操作

| 按键 | 作用 |
|------|------|
| `↑` `↓` | 列表选择 / 正文滚动 |
| `Enter` | 进入选中的问题详情(命令行为空时);否则执行命令 |
| `→` / `Tab` | 在详情页查看评论(知乎) |
| `←` / `Esc` | 返回上一级 |
| `r` | 强制刷新 / 翻下一页(知乎推荐为真翻页,股票为强制刷新行情) |
| `1`-`9` | 在编辑器打开详情页第 N 张图 |
| `/search 关键词` | 搜索(知乎) |
| `/add 代码` | 添加股票自选,如 `/add SPCX`、`/add 159941` |
| `/delete 代码` | 删除股票自选 |
| `/login` | 重新登录(粘贴新 Cookie 切换账号,覆盖旧的) |
| `/zhihu` | 知乎 |
| `/v2ex` | V2EX |
| `/hupu` | 虎扑 |
| `/nga` | NGA(需 cookie) |
| `/linuxdo` | Linux.do(需 cookie) |
| `/stock` | 股票自选(A股/美股) |
| `/books [目录]` | 打开电子书书架,首次进入时选择目录 |
| `/quit` / `q` | 退出 |

## 电子书阅读

根屏选择「电子书阅读」后按回车或 `f`，会打开系统目录选择器；也可以直接输入书库目录。程序会递归扫描并缓存目录中的 EPUB、PDF、TXT、Markdown、HTML、DOCX、ODT、RTF、FB2，以及在安装 Calibre 时通过 `ebook-convert` 读取 MOBI/AZW/AZW3。

- 书架中用 `↑↓` 选择书，回车进入章节列表；首次打开默认第一章，之后默认回到上次阅读章节。
- 章节列表用 `↑↓` 选择，回车开始阅读；阅读器中按 `Tab` / `Shift-Tab` 切换下一章 / 上一章。
- 阅读位置会按书缓存到系统缓存目录，重新打开时恢复到上次位置；正文顶部显示已读字数、本章剩余字数、当前章节和总章节数。
- 阅读器底部固定显示书名、章节、段落、已读/剩余/总字数和图片数；`-`/`－` 切换上一章，`=`/`＝`/`+`/`＋` 切换下一章，`PageUp`/`PageDown` 用于翻页。
- EPUB 内嵌图片显示为 `【图片1】`、`【图片2】`。按住 Shift 输入图片序号（支持多位数，如 `Shift+1`、`Shift+2` 选择图片 12），再按 Enter，复用知乎图片预览逻辑打开。
- `Ctrl+C` 可在任意页面直接退出程序。
- 阅读器同样支持 `c` 切换 Claude 风格伪装、`` ` `` 老板键和 `Esc` 返回。

## 多平台

支持知乎、V2EX、虎扑、NGA、Linux.do、股票自选,统一「列表 → 详情」交互。论坛帖子(V2EX/虎扑/NGA/Linux.do)的主楼与楼层回复拼成一页正文,可整页滚动。

- **V2EX / 虎扑**:无需 cookie,直接 `/v2ex`、`/hupu` 即可。
- **NGA / Linux.do**:需各自的登录 cookie。首次 `/nga`、`/linuxdo` 会进入登录流程,粘贴对应站点的 cookie 回车(NGA 需登录态,含真实 `ngaPassportUid`/`ngaPassportCid`;Linux.do 需含 `_t`/`_forum_session` 等)。各平台 cookie 独立保存。
- **股票自选**:无需 cookie。进入 `/stock` 后可用 `/add 159941` 添加 A 股、`/add SPCX` 添加美股；`/delete 代码` 删除。列表一行两列展示，A股显示实时价格与涨跌幅，美股前值显示盘前/盘后价，后值显示收盘价。默认每 60 秒自动轮询一次，按 `r` 会强制刷新。

## 配置

Cookie 明文保存在:

- macOS:`~/Library/Application Support/touch-fish/config.toml`
- Linux:`~/.config/touch-fish/config.toml`
- Windows:`%APPDATA%\touch-fish\config.toml`

```toml
[zhihu]
cookie = "..."

[nga]
cookie = "..."

[linuxdo]
cookie = "..."

[stock]
watchlist = [
  { code = "159941", name = "纳指ETF" },
  { code = "SPCX", name = "SPCX" },
]
```

美股扩展时段优先尝试本地 Python `yfinance` WebSocket 子进程。你需要有:

- `python3`
- 并可安装 `py/requirements.txt` 中的依赖 (`yfinance`)

如果本地没有 `yfinance`、或 WebSocket 失败,程序会自动回退到现有 Yahoo 方案。

## 实现说明

- 终端界面:`ratatui` + `crossterm`;异步:`tokio`;HTTP:`reqwest`。
- 知乎接口的 `x-zse-96` 签名:内嵌知乎前端的签名 JS,用 `rquickjs`(QuickJS)在 Rust 里执行生成。数据获取方案参考开源项目 `ylw1997/touchFish`。
- 签名引擎是 `!Send` 的,所以网络客户端跑在独立的 worker 线程上,UI 线程通过 channel 与之通信。

仅供学习与个人使用。请遵守知乎的使用条款,不要高频请求。
