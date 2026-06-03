# touch-fish 🐟

[![npm](https://img.shields.io/npm/v/moyu-fish.svg)](https://www.npmjs.com/package/moyu-fish)

在终端(cmd / PowerShell / macOS / Linux)里摸鱼。支持知乎、V2EX、虎扑、NGA、Linux.do,统一「列表 → 详情」交互,全程方向键浏览,界面就画在当前终端窗口里。

## 安装

npm 包地址:https://www.npmjs.com/package/moyu-fish

```bash
# 直接运行(无需安装)
npx moyu-fish

# 或全局安装(安装后命令为 moyu)
npm install -g moyu-fish
moyu
```

## 使用

1. 运行:
   ```bash
   cargo run --release
   ```
2. 启动后是平台选择列表,`↑↓` 选平台、回车进入。想免登录先体验,选 **V2EX** 或 **虎扑**(无需 cookie)。
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
| `r` | 刷新 / 翻下一页(知乎推荐为真翻页,不再重复) |
| `1`-`9` | 在编辑器打开详情页第 N 张图 |
| `/search 关键词` | 搜索(知乎) |
| `/login` | 重新登录(粘贴新 Cookie 切换账号,覆盖旧的) |
| `/zhihu` | 知乎 |
| `/v2ex` | V2EX |
| `/hupu` | 虎扑 |
| `/nga` | NGA(需 cookie) |
| `/linuxdo` | Linux.do(需 cookie) |
| `/quit` / `q` | 退出 |

## 多平台

支持知乎、V2EX、虎扑、NGA、Linux.do,统一「列表 → 详情」交互。论坛帖子(V2EX/虎扑/NGA/Linux.do)的主楼与楼层回复拼成一页正文,可整页滚动。

- **V2EX / 虎扑**:无需 cookie,直接 `/v2ex`、`/hupu` 即可。
- **NGA / Linux.do**:需各自的登录 cookie。首次 `/nga`、`/linuxdo` 会进入登录流程,粘贴对应站点的 cookie 回车(NGA 需登录态,含真实 `ngaPassportUid`/`ngaPassportCid`;Linux.do 需含 `_t`/`_forum_session` 等)。各平台 cookie 独立保存。

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
```

## 实现说明

- 终端界面:`ratatui` + `crossterm`;异步:`tokio`;HTTP:`reqwest`。
- 知乎接口的 `x-zse-96` 签名:内嵌知乎前端的签名 JS,用 `rquickjs`(QuickJS)在 Rust 里执行生成。数据获取方案参考开源项目 `ylw1997/touchFish`。
- 签名引擎是 `!Send` 的,所以网络客户端跑在独立的 worker 线程上,UI 线程通过 channel 与之通信。

仅供学习与个人使用。请遵守知乎的使用条款,不要高频请求。
