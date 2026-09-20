# 吉大校内通知（jlu-oa-notifier）

一个查看 **吉林大学校内通知** 的桌面 / 移动端应用，支持按 **发布组织** 筛选、关键词搜索、日期范围过滤、分页浏览、查看详情与下载附件。

数据来源：吉林大学电子校务平台 `https://oa.jlu.edu.cn/defaultroot/PortalInformation!jldxList.action?channelId=179577`（`channelId=179577` 即「校内通知」栏目）。

技术栈：**Tauri 2**（Rust 后端 + Web 前端），目标平台 **Windows 桌面 + Android**。

## 功能

- 📋 通知列表：标题、发布组织、发布时间、`置顶`/`新` 标记，分页浏览（共约 7.5 万条 / 2497 页）。
- 🏢 **关注组织（白名单，多选）**：勾选你关心的组织，列表只显示这些组织的通知；设置本地记住。
- 🕒 **时间范围快捷**：今天 / 近三天 / 近一周 / 全部，**默认近三天**；设置本地记住。
- 🔍 关键词搜索：按「标题 / 组织 / 内容」搜索，可与上面的关注组织、时间范围叠加。
- 📄 详情查看：完整正文（内嵌图片自动转绝对地址并在后端内联）、发布组织与时间。
- 📎 附件下载：点击附件调用系统浏览器下载（后端完成站内下载协议编码）。
- 🔗 原文跳转：在系统浏览器中打开通知原文。

## 项目结构

```
jlu-oa-notifier/
├── src/                  # 前端（Vite + TypeScript）
│   ├── main.ts           # 界面逻辑
│   ├── api.ts            # invoke 封装（调用 Rust 命令）
│   ├── orgs.ts           # 内置组织列表
│   ├── types.ts          # 类型定义
│   └── style.css         # 样式（响应式）
├── oa-core/              # Rust 核心库（抓取 + 解析，无 UI 依赖）
│   ├── src/models.rs     # 数据结构
│   ├── src/parse.rs      # HTML 解析（纯 Rust，可独立编译测试）
│   ├── src/fetch.rs      # reqwest 抓取（默认 http 特性）
│   └── examples/         # scrape_check / fetch_check 自检
└── src-tauri/            # Tauri 应用壳
    ├── src/lib.rs        # 命令封装（fetch_list / fetch_detail / search_orgs / build_attachment_url）
    ├── tauri.conf.json
    ├── capabilities/default.json
    └── icons/            # 桌面 + Android 图标
```

## 前置要求

- **Node.js** ≥ 18、**npm**
- **Rust** 稳定版（[rustup](https://rustup.rs/)）
- 桌面（Windows）构建：**Visual Studio 2022 生成工具**（含「使用 C++ 的桌面开发」工作负载）+ **WebView2**（Windows 10/11 自带）
- Android 构建：**Android Studio** + **Android SDK / NDK** + Rust 的 Android 目标

## 桌面开发与构建（Windows）

```bash
# 1. 安装依赖（会下载 Tauri CLI 对应平台的二进制）
npm install

# 2. 开发调试（热更新）
npm run tauri dev

# 3. 打包发布版（产物在 src-tauri/target/release/bundle/）
npm run tauri build
```

> Windows 首次构建需在 `~/.cargo` 或系统环境已配置好 MSVC 工具链；`tauri build` 会自动执行 `npm run build` 打包前端。

### 开发迭代：别每次都 `tauri build`

`tauri build` 慢是因为它同时做了三件重活：**release 优化编译** + 前端打包 + **生成 WiX/NSIS 安装包**。日常改代码完全不需要这些，用开发模式：

```bash
npm run dev:app        # 等价于 npm run tauri dev
```

启动一次后**保持窗口开着**：

- **Rust 只编译一次**（debug 模式，比 release 快很多）；
- 之后改**前端**（`src/` 下的 TS/CSS/HTML）→ Vite 热更新，**秒级生效，完全不重编 Rust**；
- 改 **Rust**（`oa-core/`、`src-tauri/`）→ 自动增量编译并重启窗口；
- 窗口内按 `F12` 打开 DevTools 看报错。

按场景选择命令：

| 场景 | 命令 | 耗时量级 |
| --- | --- | --- |
| 日常迭代（推荐） | `npm run dev:app` | 前端改动 **< 1s**，Rust 改动增量编译 |
| 只要 exe，不要安装包 | `npm run build:exe` | 跳过 WiX/NSIS 打包 |
| 出正式安装包 | `npm run tauri build` | 发布时才用 |

**Windows 提速技巧**

- 把**项目目录**和 **`src-tauri\target`** 加入 Windows Defender 的排除项 —— Windows 上提升非常明显。
- 可选：在 `src-tauri/Cargo.toml` 加上
  ```toml
  [profile.dev]
  debug = "line-tables-only"
  ```
  减少调试符号生成，链接阶段会快不少（代价是调试信息变少）。

### Windows 构建常见问题

**1. 报错 `linker 'link.exe' not found`**

未装 MSVC 工具链。安装 [VS Build Tools 2022](https://visualstudio.microsoft.com/zh-hans/visual-cpp-build-tools/) 并勾选「使用 C++ 的桌面开发」，再执行 `rustup default stable-x86_64-pc-windows-msvc`，重开终端。

**2. 报错 `failed to bundle project: timeout: global`（下载 WiX / NSIS 超时）**

此时 **exe 其实已经编译成功**（位于 `src-tauri\target\release\jlu-oa-notifier.exe`），失败的只是「打成安装包」这一步：Tauri 需要从 GitHub 下载 WiX / NSIS 打包工具，但网络超时。可选：

- **不要安装包，直接用 exe**：双击 `jlu-oa-notifier.exe` 即可运行（等价绿色免安装版）。以后只编译 exe：
  ```powershell
  npm run tauri build -- --no-bundle
  ```
- **挂代理后重试**（可以出安装包）：
  ```powershell
  $env:HTTPS_PROXY="http://127.0.0.1:7890"   # 换成你的代理端口
  $env:HTTP_PROXY="http://127.0.0.1:7890"
  npm run tauri build
  ```
- **只打 NSIS 安装包**（不触发 WiX）：
  ```powershell
  npm run tauri build -- --bundles nsis
  ```
- **离线手动放置工具**：把工具放进 Tauri 缓存目录 `%LOCALAPPDATA%\tauri\`
  - WiX（打 MSI 用）：下载 `wix314-binaries.zip`，解压后让 `candle.exe`、`light.exe` 等**直接**位于
    `%LOCALAPPDATA%\tauri\WixTools314\`
  - NSIS（打 exe 安装包用）：下载 `nsis-3.11.zip`，把解压出的 `nsis-3.11` 文件夹**重命名为 `NSIS`**，
    使 `makensis.exe` 直接位于 `%LOCALAPPDATA%\tauri\NSIS\`；再把 `nsis_tauri_utils.dll` 放到
    `%LOCALAPPDATA%\tauri\NSIS\Plugins\x86-unicode\additional\`

  官方下载地址：
  - WiX：`https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip`
  - NSIS：`https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip`
  - NSIS 插件：`https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll`

  国内网络可在上述 URL 前加 GitHub 加速前缀（如 `https://ghproxy.net/`）再下载。

**3. 报错 `failed to run ...\WixTools314\light.exe`（中文名导致 MSI 打包失败）**

现象：WiX 已下载、`candle` 也跑过，但 `light.exe` 失败。原因是 MSI 默认用 `en-US` 语言（代码页 **1252**），
**无法编码中文**的 `productName` / `shortDescription`，于是 `light.exe` 生成 MSI 时报错。

修复：在 `src-tauri/tauri.conf.json` 指定 WiX 使用中文语言（代码页 **936**）：

```json
"bundle": {
  "windows": {
    "wix": { "language": "zh-CN" }
  }
}
```

（本项目已配置好。）其它可选方案：

- 改用 NSIS 打包（Unicode，天然支持中文）：`npm run tauri build -- --bundles nsis`
- 或把 `productName` 改成纯 ASCII（如 `JLU-OA-Notifier`），窗口标题仍可保持中文

**4. 依赖下载慢**

- npm：`npm config set registry https://registry.npmmirror.com`
- cargo：在 `C:\Users\<你>\.cargo\config.toml` 写入
  ```toml
  [source.crates-io]
  replace-with = 'rsproxy-sparse'
  [source.rsproxy-sparse]
  registry = "sparse+https://rsproxy.cn/index/"
  ```

## 移动端（Android）开发与测试

### 最快的布局预览（不用 Android 环境）

界面是响应式的，**760px** 是断点。直接把桌面窗口拖窄（窗口 `minWidth` 是 360），
或用 `npm run dev:app` 后在窗口内按 `F12` 打开 DevTools 切换设备模拟，即可预览单栏布局、
组织面板、时间分段控件在窄屏下的效果。**但这只验证布局，不验证真机行为。**

### 前置环境（一次性）

**Tauri 2.11 的 Android 构建对版本有硬性要求**（从 tauri-cli 二进制中确认）：

| 组件 | 要求 |
| --- | --- |
| `compileSdk` / `targetSdk` | **36**（即 SDK Platform `android-36`） |
| `minSdk` | 21 |
| **NDK** | **29.0.13846066**（必须是这个精确版本） |
| Android Gradle Plugin | 8.11.0 |
| Gradle | 8.14.3 |
| 读取的环境变量 | `JAVA_HOME`、`ANDROID_HOME`（或 `ANDROID_SDK_ROOT`）、`NDK_HOME`、`GRADLE_USER_HOME` |

#### 装法 A：让 Tauri 自动装（先试这个）

只要装好 **Android Studio** 并设好 `ANDROID_HOME`，`npm run android:init` 会自动调用
`sdkmanager` 帮你装 `platform-tools`、`platforms;android-36` 和 `ndk;29.0.13846066`。

#### 装法 B：手动用 SDK Manager 装（网络更可控）

Android Studio → **SDK Manager** →

- **SDK Platforms**：勾 `Android 16 (API 36)`
- **SDK Tools**：勾 `Android SDK Build-Tools`、`Android SDK Command-line Tools`、`Platform-Tools`、
  **`NDK (Side by side)` 勾选并确保版本为 `29.0.13846066`**

或用命令行（`sdkmanager` 在 `%ANDROID_HOME%\cmdline-tools\latest\bin`）：

```powershell
sdkmanager --install "platform-tools" "platforms;android-36" "build-tools;36.0.0" `
           "ndk;29.0.13846066" "cmdline-tools;latest"
```

#### 1) Rust 的 Android 目标

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

真机（现代手机）只需 `aarch64-linux-android`；模拟器（x86 镜像）需要 `x86_64-linux-android`。

#### 2) 环境变量（Windows，永久生效）

```powershell
[Environment]::SetEnvironmentVariable("ANDROID_HOME", "$env:LOCALAPPDATA\Android\Sdk", "User")
[Environment]::SetEnvironmentVariable("NDK_HOME",     "$env:LOCALAPPDATA\Android\Sdk\ndk\29.0.13846066", "User")
[Environment]::SetEnvironmentVariable("JAVA_HOME",    "C:\Program Files\Android\Android Studio\jbr", "User")
# 把 platform-tools（adb）加进 PATH
$p = [Environment]::GetEnvironmentVariable("Path", "User")
[Environment]::SetEnvironmentVariable("Path", "$p;$env:LOCALAPPDATA\Android\Sdk\platform-tools", "User")
```

> 改完**重开终端**；`JAVA_HOME` 用 Android Studio 自带的 `jbr`（就是 JDK 17），比单独装 JDK 省事。
> 只想当前会话临时试：把上面的 `[Environment]::...` 换成 `$env:XXX = "..."`。

#### 3) 国内网络加速（很关键）

Android 构建要下三样东西，默认源在国内都很慢：

1. **Gradle 发行包**（`gradle-8.14.3-bin.zip` @ services.gradle.org）
   编辑 `src-tauri/gen/android/gradle/wrapper/gradle-wrapper.properties`，把 `distributionUrl`
   换成国内镜像（如 `https://mirrors.cloud.tencent.com/gradle/gradle-8.14.3-bin.zip`）。
   ⚠️ 该文件在生成目录里，重跑 `android:init` 会被覆盖。
2. **Maven 依赖**（Google / Maven Central）→ 用**全局** `%USERPROFILE%\.gradle\init.gradle`，
   不要改生成目录里的 gradle 文件：
   ```groovy
   allprojects {
       repositories {
           maven { url 'https://maven.aliyun.com/repository/google' }
           maven { url 'https://maven.aliyun.com/repository/central' }
           maven { url 'https://maven.aliyun.com/repository/gradle-plugin' }
           maven { url 'https://maven.aliyun.com/repository/public' }
       }
   }
   ```
3. **SDK / NDK 组件**（dl.google.com）→ 挂代理，或改用装法 B 在 Android Studio 里手动装。

#### 4) 装完自检

```powershell
java -version                                  # 应为 17.x
adb version                                    # 能输出版本号说明 PATH 配好了
sdkmanager --list_installed                    # 应包含 platform-tools / platforms;android-36 / ndk;29.0.13846066
rustup target list --installed                 # 应包含 aarch64-linux-android
```

#### 5) 常见报错对照

| 报错 | 原因 / 处理 |
| --- | --- |
| `Failed to install Android SDK` | CLI 找不到 `sdkmanager` 或 `ANDROID_HOME` 不对 → 改用装法 B 手动装 |
| `No version of NDK matched the requested version 29.0.13846066` | NDK 版本不一致 → 装该精确版本；或改 `src-tauri/gen/android/app/build.gradle.kts` 里的 `ndkVersion` |
| `SDK location not found` | `ANDROID_HOME` 未生效；可在 `src-tauri/gen/android/local.properties` 写 `sdk.dir=D:\\Android\\Sdk` |
| `Unsupported class file major version` / `JAVA_HOME is not set` | JDK 版本不对 → `JAVA_HOME` 指向 Android Studio 的 `jbr` |
| Gradle 下载卡住 | 见上面第 3 条镜像配置 |

#### 6) `android:dev` 报 `Failed to assemble APK ... exited with code 1` 怎么办

**Tauri 会把 Gradle 的真实报错吞掉**，只留一个 `exited with code 1`（那串 `Io(...)` 只是环境变量转储，没有诊断价值）。要拿到真正原因：

```powershell
# 方式一（最直接）：绕开 Tauri，直接跑 Gradle 看原始报错
cd D:\jlu-oa-notifier-src\src-tauri\gen\android
.\gradlew.bat assembleDebug --stacktrace

# 方式二：让 Tauri 输出详细日志
npm run tauri android dev --verbose
```

拿到报错后对照：

| Gradle 报错关键字 | 原因 | 处理 |
| --- | --- | --- |
| `Failed to find target with hash string 'android-36'` | 没装 SDK Platform 36 | `sdkmanager --install "platforms;android-36"` |
| `Could not resolve ...` / `Connection timed out` | Maven 依赖下载失败 | 配 aliyun 镜像（见第 3 节） |
| `SDK location not found` | 缺 `sdk.dir` | `src-tauri\gen\android\local.properties` 写 `sdk.dir=D:\\Android\\Sdk` |
| `Unsupported class file major version` | JDK 版本不匹配 | `JAVA_HOME` 指向 Android Studio 的 `jbr`（JDK 17） |
| `No version of NDK matched` | NDK 版本不符 | 装 `29.0.13846066` |
| `Task '...' not found` / 工程结构异常 | `gen/android` 不完整 | 删掉 `src-tauri\gen\android`，重跑 `npm run android:init` |

顺手做一次自检（四条都应对得上）：

```powershell
java -version
echo "JAVA_HOME=$env:JAVA_HOME"
sdkmanager --list_installed | Select-String "android-36|build-tools|ndk;29"
type src-tauri\gen\android\local.properties
dir  src-tauri\gen\android\app\src\main\jniLibs
```

#### 7) Gradle 报 `SSLHandshakeException: PKIX path building failed`

典型输出：

```
Downloading https://services.gradle.org/distributions/gradle-8.14.3-bin.zip
Exception in thread "main" javax.net.ssl.SSLHandshakeException:
  PKIX path building failed: ... unable to find valid certification path to requested target
```

**含义**：Gradle wrapper 下发行包时 TLS 证书校验失败。注意报错栈里有 `HttpsClient.followRedirect` ——
`services.gradle.org` 会 302 跳到 CDN，是**跳转后那个域名**的证书不被 JDK 信任。

⚠️ **Java 有自己独立的信任库（`<JDK>\lib\security\cacerts`），和 Windows / 浏览器不是一套**，
所以「浏览器能打开」不代表 Java 能下。常见诱因：校园网/单位代理或杀软的 HTTPS 中间人、
CDN 证书链不全、JDK 信任库过旧。

**方案 1（首选）：换成国内镜像**（已确认文件存在，且更快）

编辑 `src-tauri\gen\android\gradle\wrapper\gradle-wrapper.properties`：

```properties
distributionUrl=https\://mirrors.cloud.tencent.com/gradle/gradle-8.14.3-bin.zip
```

备用：`https\://mirrors.huaweicloud.com/gradle/gradle-8.14.3-bin.zip`

> - 保留 `https\:` 里的**反斜杠转义**（properties 文件格式要求）。
> - 该文件在**生成目录**里，重跑 `android:init` 会被覆盖，需要重新改。

**方案 2（最稳）：手动下载 + 用 `file:` 地址**

用浏览器下载 `gradle-8.14.3-bin.zip` 到本地，然后：

```properties
distributionUrl=file\:/D:/downloads/gradle-8.14.3-bin.zip
```

wrapper 就完全不联网了。

**方案 3：如果是代理/杀软做了 HTTPS 中间人（方案 1 也报同样错）**

把它的根证书导入 JDK 信任库：

```powershell
keytool -importcert -trustcacerts -alias myproxy -file proxy-ca.cer `
        -keystore "$env:JAVA_HOME\lib\security\cacerts" -storepass changeit
```

（根证书可在 `certmgr.msc → 受信任的根证书颁发机构` 里找到并导出。）

> 同类问题也会出现在后续 Maven 依赖下载上，所以第 3 节的 aliyun 镜像也一并配好。


### 跑起来

```bash
npm run android:init   # 一次性：生成原生工程 src-tauri/gen/android
npm run android:dev    # 编译 debug APK → 安装到设备/模拟器（前端热更新）
```

`android:dev` 期间改前端（`src/` 下的 TS/CSS）会自动热更新，和桌面端 `dev:app` 一样。

### 用哪个「设备」

| 方式 | Rust 目标 | 特点 |
| --- | --- | --- |
| **模拟器**（Android Studio 建 AVD，选 x86_64 镜像） | `x86_64-linux-android` | 启动快、可截图/录屏，走 PC 的网络 |
| **真机**（开启 USB 调试，`adb devices` 能看到） | `aarch64-linux-android` | **必须用真机验证的**：触摸手感、系统字体回退、性能、以及**校园网/VPN 可达性** |

> ⚠️ 模拟器用的是你 PC 的网络出口。如果 `oa.jlu.edu.cn` 需要校园网/VPN，**模拟器上大概率连不上**，
> 这种情况请用真机连校园网测试。

### 打包

APK / AAB 的发布打包与签名见下方「[发布安装包（Windows / Android）](#发布安装包windows--android)」。

### 真机调试：连上设备 + 看日志 / Console

**1) 连上手机**

1. 手机开启**开发者选项**（设置 → 关于手机 → 连点「版本号」7 次）
2. 打开 **USB 调试**（部分国产机还需打开「USB 安装」「USB 调试(安全设置)」）
3. 数据线连接，手机端选「**传输文件 / MTP**」（不要「仅充电」）
4. 电脑上确认：

   ```bash
   adb devices
   # 正常       <序列号>  device
   # 未授权     <序列号>  unauthorized → 手机上点「允许 USB 调试」
   # 列表为空   → 换数据线 / 换直连 USB 口（别用 HUB），或 adb kill-server && adb start-server
   ```

**2) 跑起来**

```bash
npm run android:init      # 一次性：生成原生工程
npm run android:dev       # 编译 debug APK → 安装 → 启动
```

多台设备时指定设备名（`adb devices` 里的序列号）：

```bash
npm run tauri android dev -- <设备名>
```

> `android dev` 编的是 **debug** 包，wry 会调用 `setWebContentsDebuggingEnabled(true)`，
> 所以下面的 DevTools 才能挂上去；跑 `--release` 时该开关关闭。

**3) 看 Console / DOM / 网络（前端调试）**

手机保持连接，在电脑的 **Edge / Chrome** 地址栏打开：

```
edge://inspect/#devices        （或 chrome://inspect/#devices）
```

找到 `cn.jlu.oa.notifier` 的 WebView → 点 **inspect**，得到完整 DevTools（Console、Elements、Network、Sources）。

**4) 看 Rust 日志（后端调试）**

Tauri 把 Rust 的 `println!` / panic 转发到 logcat，标签是 **`RustStdoutStderr`**：

```bash
adb logcat -c                       # 先清空
adb logcat -s RustStdoutStderr      # 只看 Rust 输出
```

按进程看应用全部日志（包名 = `tauri.conf.json` 的 `identifier`）：

```bash
adb logcat --pid=$(adb shell pidof cn.jlu.oa.notifier)
```

**5) 改代码后的行为**

- 改**前端**（`src/`）→ Vite 热更新，手机上界面秒级刷新；
- 改 **Rust** → 自动增量编译并重新安装启动（debug，比 release 快很多）。

**6) 装 APK 到别人的手机**

```bash
npm run android:apk
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/release/*.apk
```

**常见坑**

| 现象 | 处理 |
| --- | --- |
| `INSTALL_FAILED_UPDATE_INCOMPATIBLE` | 手机上已有签名不同的旧包：`adb uninstall cn.jlu.oa.notifier` |
| Gradle 下载卡住 | 给 Gradle 配阿里云等国内镜像，或挂代理 |
| 能装上但通知打不开 | 手机本身访问不了 `oa.jlu.edu.cn` —— 连校园网 / VPN（模拟器走 PC 网络，真机走手机网络） |
| 改前端手机无反应 | 确认手机与 PC 网络互通（真机走 `TAURI_DEV_HOST`），或重跑 `android:dev` |

### 移动端重点测什么

1. **Android 返回键 / 返回手势**：在详情页按返回应回到列表，在列表页按返回才退出应用
   （已用 History API + `onBackButtonPress` 实现，**需真机确认**）。
2. **正文字体**：Android 没有 `仿宋`/`宋体`，会走 `@font-face` 的 `local()` 回退到 Noto CJK —— 看是否可读、是否统一。
3. **正文图片**：详情里的图片是由后端抓取后内联的 `data:` URL，确认能显示。
4. **组织多选面板**：窄屏下弹出位置、勾选手感、是否被软键盘顶掉。
5. **附件下载**：点了应由系统浏览器接管下载。
6. **下拉/滚动**：列表与详情两个容器各自滚动，注意安卓上是否有滚动穿透。


## 发布安装包（Windows / Android）

### 一键脚本 `scripts/release.ps1`

会自动**校验版本号三处一致** → 调用 Tauri 打包 → 列出产物路径与体积 → 打印发布提醒。

```powershell
# Windows：MSI + NSIS 都出
.\scripts\release.ps1

# 只要 NSIS 安装器（日常分发推荐）
.\scripts\release.ps1 -Bundle nsis

# Android APK（只编 64 位 ARM，包更小）
.\scripts\release.ps1 -Platform android -AndroidTarget apk -Slim

# 两个平台一起打，完成后打开产物目录
.\scripts\release.ps1 -Platform all -Bundle nsis -AndroidTarget apk -Slim -Open
```

也可走 npm：`npm run release -- -Bundle nsis`

| 参数 | 取值 | 默认 |
| --- | --- | --- |
| `-Platform` | `windows` / `android` / `all` | `windows` |
| `-Bundle` | `both` / `nsis` / `msi` | `both` |
| `-AndroidTarget` | `apk` / `aab` / `both` | `apk` |
| `-Slim` | 开关：Android 只编 `aarch64` | 关 |
| `-SkipChecks` | 开关：跳过版本号与环境检查 | 关 |
| `-Open` | 开关：完成后打开产物目录 | 关 |

> 若被执行策略拦住，用 `powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1`，
> 或先 `Set-ExecutionPolicy -Scope Process Bypass`。

### 发布前检查

- **版本号三处保持一致**：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`（当前均为 `0.1.0`），每次发布递增。
- **Windows 的 `productName` 不要随意改**：MSI 的 `UpgradeCode` 由 `productName` 派生，改了会被 Windows 当成另一个应用，导致重复安装。
- **Android 包名** = `tauri.conf.json` 的 `identifier`（`cn.jlu.oa.notifier`），**发布后不可更改**。

### Windows

```powershell
npm run tauri build                      # MSI + NSIS 两种安装包
npm run tauri build -- --bundles nsis    # 只要 setup.exe
npm run tauri build -- --bundles msi     # 只要 .msi
```

产物在 `src-tauri\target\release\bundle\`：

| 文件 | 说明 |
| --- | --- |
| `nsis\吉大校内通知_0.1.0_x64-setup.exe` | NSIS 安装器，**默认按当前用户安装、不需要管理员**，适合日常分发 |
| `msi\吉大校内通知_0.1.0_x64_zh-CN.msi` | Windows Installer 包，适合企业批量部署 / 组策略 / 静默安装 |
| `..\jlu-oa-notifier.exe` | 裸可执行文件，免安装，直接拷走即可运行 |

**⚠️ 未做代码签名会有 SmartScreen 警告**

别人安装时会看到「Windows 已保护你的电脑 / 未知发布者」。这不影响功能，但要提前告知用户点
「更多信息 → 仍要运行」。想彻底消除需购买代码签名证书（OV/EV）。

**WebView2 依赖**

默认 `webviewInstallMode = downloadBootstrapper`：安装时联网拉 WebView2（Win10/11 一般已自带）。
目标机器不能联网时改成离线安装包（体积会大约 150MB）：

```json
"bundle": { "windows": { "webviewInstallMode": { "type": "offlineInstaller" } } }
```

**可选：减小体积**

当前没有 `[profile.release]`，用的是 Cargo 默认（编译快、体积大）。发布前可在 `src-tauri/Cargo.toml` 加：

```toml
[profile.release]
codegen-units = 1
lto = true
opt-level = "s"
strip = true
```

代价是编译明显变慢（几分钟），日常开发别开。

### Android

**1) 必须先签名**（未签名无法分发，也无法覆盖升级）

首次执行 `tauri android build` 时 Tauri 会交互式引导创建/配置签名；也可手动准备：

```powershell
keytool -genkey -v -keystore jlu-oa.keystore -alias jlu-oa -keyalg RSA -keysize 2048 -validity 10000
```

然后创建 `src-tauri\gen\android\keystore.properties`：

```properties
storeFile=D:/keys/jlu-oa.keystore
storePassword=你的密码
keyAlias=jlu-oa
keyPassword=你的密码
```

> **务必备份 keystore 文件和密码**。丢了就再也无法给已发布的 app 出升级包，只能换包名重新上架。

**2) 构建**

```powershell
npm run tauri android build -- --apk                 # APK（国内分发用）
npm run tauri android build -- --aab                 # AAB（Google Play 上架用）
npm run tauri android build -- --apk --aab           # 两个都出
npm run tauri android build -- --apk -t aarch64      # 只编 64 位 ARM，包更小
npm run tauri android build -- --apk --split-per-abi # 按 ABI 拆成多个包
npm run tauri android build -- --apk --ci            # CI 环境：跳过交互式提问
```

产物在 `src-tauri\gen\android\app\build\outputs\`（`apk\` 或 `bundle\`）：

```powershell
dir /s /b src-tauri\gen\android\app\build\outputs\*.apk
```

**3) APK 还是 AAB**

| | APK | AAB |
| --- | --- | --- |
| 用途 | 直接安装、国内应用商店、官网分发 | Google Play 上架 |
| 能否直接装 | 能 | 不能（需走 Play 或 bundletool） |

国内场景一般**只需要 APK**。默认 universal 包含 4 种 ABI，用 `-t aarch64` 或 `--split-per-abi` 可显著减小体积。

**4) 用 release 包在真机验一遍**

`release` 构建会**关闭 WebView 调试**（devtools 只在 debug 开启），行为可能与 `dev` 不同 ——
装上 release 包确认能正常打开站内通知、图片显示、字体正常。

## 核心接口（Rust 命令）

| 命令 | 参数 | 说明 |
| --- | --- | --- |
| `fetch_list` | `{ org?, page?, keyword?, searchType?, dateRange? }` | 拉取列表；`searchType` 0=标题 1=组织 2=内容，`dateRange` ""/1/6/12 |
| `fetch_detail` | `{ id }` | 拉取详情（标题/时间/组织/正文/附件） |
| `search_orgs` | `{ query }` | 按组织名模糊搜索 |
| `build_attachment_url` | `{ informationId, filename, name }` | 生成附件下载直链 |

## 说明与限制

- **网络环境**：`oa.jlu.edu.cn` 为校内平台，通常需在校园网或 VPN 环境下访问。
- **关注组织与时间范围（客户端过滤）**：站方接口有两条硬限制，决定了这两个功能只能在客户端做：
  1. `searchDate`（近一月/半年/一年）**必须配合关键词才生效**，单独传日期不过滤；
  2. `orgname` 只接受**单个**组织，无法一次筛多个。

  因此实现方式是：
  - **今天 / 近三天 / 近一周** → 从第 1 页起自动连续抓取，直到该页（忽略置顶项）已越过时间线为止（实测：今天 1 页、近三天 2 页、近一周 4 页），再本地过滤。上限分别为 5 / 12 / 25 页。
  - **全部** → 沿用服务端分页；若恰好关注 **1 个**组织则交给服务端 `orgname`（分页最准），**≥2 个**时本地过滤，并自动跳过过滤后为空的页（最多连跳 5 页）。
  - 「今天」按 **Asia/Shanghai** 时区判定，不依赖本机时区；站方只提供 `今天 HH:MM` / `昨天 HH:MM` / `YYYY-MM-DD` 这几种时间文本，无法识别的按「不过滤」处理（宁可多显示不漏掉）。
  - **置顶通知豁免时间过滤**：始终保留并排在最前面（与官网一致），因此「今天」视图里可能看到几天前的置顶通知。
  - 关注组织与时间范围保存在本地 `localStorage`，不联网、不上传。
- **附件下载**：通过系统浏览器打开下载直链完成，附件是否可下载取决于平台权限与登录态（本站附件当前匿名可下载）。
- **置顶/新标记**：解析自列表页的 `[置顶]` 与 `new.gif` 标记。
- **内容渲染**：详情正文为站方 HTML，应用通过 `innerHTML` 注入并统一把相对图片/链接转为绝对地址；应用关闭了 CSP（`csp: null`）以便加载站内图片，如需更严格可自行配置。
- **正文图片走后端**：正文图片（`<img src="/defaultroot/upload/html/xxx.png">`）不交给 WebView 直接加载，而是由 Rust 命令 `fetch_image` 抓取后内联为 `data:` URL。原因是 app 前端运行在 `tauri://localhost`，与 `https://oa.jlu.edu.cn` 跨源，WebView 直接加载会受跨源策略 / 证书信任 / 代理环境差异影响而失败。该命令只允许抓取 `https://oa.jlu.edu.cn` 下的地址（防 SSRF）；失败时前端保留原始地址兜底。
- **正文字体归一化**：通知正文是 Word 导出的 HTML，字号是绝对值且各篇不一致（常见 `9pt`≈12px、`14pt`≈18.7px、`15pt`、`16pt`），字体名也五花八门（`仿宋`/`仿宋_GB2312`/`方正仿宋_GB2312`/`宋体`/`黑体`，后两个 Windows 默认未安装）。应用做了两件事：① 用 `@font-face` + `local()` 做字体别名，缺失字体回退到本机等价 CJK 字体；② 注入正文后把小于 `15px` 的字号抬到下限（`src/main.ts` 的 `normalizeFontSizes`，上标/下标除外），避免个别通知字号过小。

## 验证

`oa-core` 采用「解析 / 抓取」分层，解析部分为纯 Rust，可离线自检：

```bash
# 解析逻辑离线自检（读取本地 HTML）
cargo run -p oa-core --example scrape_check --no-default-features -- <列表HTML> <详情HTML>

# 在线抓取自检（对真实网站执行列表/详情/组织搜索/附件直链）
cargo run -p oa-core --example fetch_check
```

`scripts/verify/verify.mjs` 用 cheerio 复现了与 Rust 后端相同的选择器，可对照验证解析结果。

`scripts/verify/feed_check.mjs` 对真实数据校验「今天 / 近三天 / 近一周」的抓取与过滤逻辑（抓多少页、命中多少条）：

```bash
node scripts/verify/feed_check.mjs
```

## 图标

应用图标由 `icon-source.png`（1024×1024，JLU 蓝底白色通知圆点）通过 `npx tauri icon icon-source.png` 生成到 `src-tauri/icons/`。修改图标后重新执行该命令即可。
