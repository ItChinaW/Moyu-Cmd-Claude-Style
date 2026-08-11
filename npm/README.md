# moyu-fish

终端里的摸鱼客户端，支持知乎、V2EX、虎扑、NGA、Linux.do、股票行情和本地电子书阅读。

## 安装

```bash
npm install -g moyu-fish
moyu
```

调试当前源码：

```bash
moyu-test
```

## 功能

- Claude Code 风格终端界面和老板键
- 知乎、V2EX、虎扑、NGA、Linux.do 内容浏览
- 自选股票行情和 Yahoo WebSocket 实时更新
- EPUB、MOBI、TXT、Markdown、HTML、DOCX、ODT、RTF、FB2、PDF 阅读
- EPUB/MOBI 图片提取和阅读进度缓存

## 电子书

启动后选择“电子书阅读”，输入书籍目录即可递归加载文件。

- `↑` / `↓`：移动
- `Enter`：打开
- `-` / `=`：上一章 / 下一章
- `PageUp` / `PageDown`：翻页
- `Ctrl+C`：退出

扫描版 PDF 没有文字层，暂不支持正文提取。

## 源码

项目地址：[ItChinaW/Moyu-Cmd-Claude-Style](https://github.com/ItChinaW/Moyu-Cmd-Claude-Style)

完整说明见仓库中的 [`使用说明.md`](https://github.com/ItChinaW/Moyu-Cmd-Claude-Style/blob/master/%E4%BD%BF%E7%94%A8%E8%AF%B4%E6%98%8E.md)。
