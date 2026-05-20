# Kibble

A pixel cat sits in a small window on your desktop. Drag a food photo onto it, the cat chews, an old paper scroll slides out for you to scribble a note, and the photo is committed and pushed to your private "kibble" git repository. That's the whole app.

![screenshot](docs/screenshot.png)

## Why

Logging meals shouldn't be a form. It should be a gesture. Kibble is the gesture; everything else (classification, EXIF, nutrition) belongs in the Skill that reads the repo later.

## Quick start

### 1. Prepare your data repository

```bash
# create a private repo on GitHub (or wherever), then:
git clone git@github.com:you/kibble-data.git ~/kibble-data
```

Make sure `git push` works in that directory before launching Kibble. Kibble does not handle authentication.

### 2. Configure Kibble

Run Kibble once. It will create a config template at `~/.config/kibble/config.toml` (or `%APPDATA%\kibble\config.toml` on Windows) and print the path to the console.

Edit `repo_path` to point at your clone:

```toml
repo_path = "/Users/you/kibble-data"
```

### 3. Build & run

```bash
cd app
pnpm install
pnpm tauri dev      # development
pnpm tauri build    # production binary
```

## How it works

1. Drop image(s) onto the cat.
2. Cat chews. A scroll slides out. Type a note (or don't). Press Enter.
3. Kibble copies the images into `<repo_path>/inbox/`, writes a sibling `.note.txt` if you wrote a note, and runs `git add -A && git commit && git push` in that directory.
4. Cat looks satisfied on success, confused on failure. Errors go to stderr.

The downstream Skill (see `skill/SKILL.md`) is responsible for everything else: parsing EXIF, classifying meal type from timestamp, identifying food, generating summaries.

## File layout in the data repo

```
kibble-data/
└── inbox/
    ├── IMG_1234.jpg
    ├── IMG_1234.jpg.note.txt    # only if you wrote a note
    └── IMG_1235.jpg
```

Filenames are preserved. Collisions get a `_1`, `_2` suffix.

## License

MIT

---

# Kibble (中文)

桌面上一个小窗口里坐着一只像素猫。把吃的照片拖到它身上，猫嚼一嚼，一卷羊皮纸从底部滑出来让你写备注，然后照片就被 commit + push 到你的私有 "kibble" 数据仓库里。整个 App 就这么多。

![screenshot](docs/screenshot.png)

## 为什么

记录饮食不应该是填表单，应该是一个动作。Kibble 提供这个动作，其他的事（分类、EXIF、营养分析）交给后续读这个 repo 的 Skill。

## 快速开始

### 1. 准备数据仓库

```bash
# 在 GitHub（或别的地方）建一个私有 repo，然后：
git clone git@github.com:you/kibble-data.git ~/kibble-data
```

确认在该目录下 `git push` 可以直接成功，再启动 Kibble。Kibble 不处理认证。

### 2. 配置 Kibble

首次运行 Kibble，会自动在 `~/.config/kibble/config.toml`（Windows 上是 `%APPDATA%\kibble\config.toml`）生成一个带注释的模板，并在控制台输出路径。

编辑 `repo_path`，指向你的本地仓库：

```toml
repo_path = "/Users/you/kibble-data"
```

### 3. 构建 & 运行

```bash
cd app
pnpm install
pnpm tauri dev      # 开发模式
pnpm tauri build    # 打包二进制
```

## 工作流程

1. 把图片拖到猫上。
2. 猫开始嚼，卷轴滑出。写备注（或者不写）。按 Enter。
3. Kibble 把图片复制到 `<repo_path>/inbox/`，如果有备注就写一个同名 `.note.txt`，然后在该目录里 `git add -A && git commit && git push`。
4. 成功猫露出满足表情，失败猫一脸困惑。详细错误打到 stderr。

下游的 Skill（见 `skill/SKILL.md`）负责其他所有事情：解析 EXIF、根据时间戳判断 meal type、识别食物、生成总结。

## 数据仓库里的文件结构

```
kibble-data/
└── inbox/
    ├── IMG_1234.jpg
    ├── IMG_1234.jpg.note.txt    # 只有当你写了备注时才有
    └── IMG_1235.jpg
```

原文件名保留。重名追加 `_1`、`_2` 后缀。

## License

MIT
