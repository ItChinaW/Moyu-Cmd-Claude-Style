# touch-fish 🐟

在终端(cmd / PowerShell / macOS / Linux)里摸鱼。v1 支持知乎:热榜、搜索、回答详情、评论,全程方向键浏览,界面就画在当前终端窗口里。

## 使用

1. 运行:
   ```bash
   cargo run --release
   ```
2. 在底部命令行输入 `/zhihu` 回车进入知乎。
   首次使用会进入登录流程,需要粘贴你的知乎 Cookie:
   登录 zhihu.com → 按 **F12** → **Network** 标签 → 刷新 → 点任意一个 `www.zhihu.com` 请求 → 在 **Request Headers** 里复制 `cookie:` 整行的值 → 粘贴到命令行回车。
   程序会发一个测试请求验证 Cookie,通过后保存到本地配置,之后启动直接进热榜。

## 操作

| 按键 | 作用 |
|------|------|
| `↑` `↓` | 列表选择 / 正文滚动 |
| `Enter` | 进入选中的问题详情(命令行为空时);否则执行命令 |
| `→` / `Tab` | 在详情页查看评论 |
| `←` / `Esc` | 返回上一级 |
| `/search 关键词` | 搜索 |
| `/login` | 重新登录(粘贴新 Cookie 切换账号,覆盖旧的) |
| `/zhihu` | 进入知乎热榜 |
| `/quit` / `q` | 退出 |

## 配置

Cookie 明文保存在:

- macOS:`~/Library/Application Support/touch-fish/config.toml`
- Linux:`~/.config/touch-fish/config.toml`
- Windows:`%APPDATA%\touch-fish\config.toml`

```toml
[zhihu]
cookie = "..."
```

## 实现说明

- 终端界面:`ratatui` + `crossterm`;异步:`tokio`;HTTP:`reqwest`。
- 知乎接口的 `x-zse-96` 签名:内嵌知乎前端的签名 JS,用 `rquickjs`(QuickJS)在 Rust 里执行生成。数据获取方案参考开源项目 `ylw1997/touchFish`。
- 签名引擎是 `!Send` 的,所以网络客户端跑在独立的 worker 线程上,UI 线程通过 channel 与之通信。

仅供学习与个人使用。请遵守知乎的使用条款,不要高频请求。
