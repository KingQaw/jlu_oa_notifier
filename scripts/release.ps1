#Requires -Version 5.1
<#
.SYNOPSIS
  一键发布「吉大校内通知」的安装包（Windows / Android）。

.DESCRIPTION
  发布前自动校验版本号是否三处一致，然后调用 Tauri CLI 打包，
  最后列出产物路径与体积，并打印发布提醒。

.EXAMPLE
  .\scripts\release.ps1
  打包 Windows 的 MSI + NSIS 安装包。

.EXAMPLE
  .\scripts\release.ps1 -Bundle nsis
  只打 Windows 的 NSIS 安装器（日常分发推荐，无需管理员权限）。

.EXAMPLE
  .\scripts\release.ps1 -Platform android -AndroidTarget apk
  打包 Android APK。

.EXAMPLE
  .\scripts\release.ps1 -Platform android -AndroidTarget apk -Slim
  只编 64 位 ARM（aarch64），包更小。

.EXAMPLE
  .\scripts\release.ps1 -Platform all -Bundle nsis -AndroidTarget apk -Slim -Open
  两个平台都打，完成后打开产物目录。
#>
[CmdletBinding()]
param(
    [ValidateSet("windows", "android", "all")]
    [string]$Platform = "windows",

    [ValidateSet("both", "nsis", "msi")]
    [string]$Bundle = "both",

    [ValidateSet("apk", "aab", "both")]
    [string]$AndroidTarget = "apk",

    # Android：只编 64 位 ARM，显著减小体积
    [switch]$Slim,

    # 跳过版本号与工具检查（不建议）
    [switch]$SkipChecks,

    # 结束后打开产物所在目录
    [switch]$Open
)

$ErrorActionPreference = "Stop"

function Write-Step($msg) { Write-Host ""; Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "    $msg" -ForegroundColor Green }
function Write-Warn2($msg){ Write-Host "    $msg" -ForegroundColor Yellow }
function Write-Err2($msg) { Write-Host "    $msg" -ForegroundColor Red }

# 切到仓库根目录（本脚本位于 <repo>\scripts\）
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repoRoot
Write-Host "仓库根目录: $repoRoot" -ForegroundColor DarkGray

$script:Version = "unknown"

# ---------- 版本号 ----------
function Get-JsonVersion($path) {
    $json = Get-Content -LiteralPath $path -Raw -Encoding UTF8 | ConvertFrom-Json
    return [string]$json.version
}

function Get-CargoVersion($path) {
    $text = Get-Content -LiteralPath $path -Raw -Encoding UTF8
    $m = [regex]::Match($text, '(?m)^\s*version\s*=\s*"([^"]+)"')
    if (-not $m.Success) { throw "在 $path 中找不到 version 字段" }
    return $m.Groups[1].Value
}

function Test-VersionConsistency {
    Write-Step "校验版本号一致性"

    $targets = @(
        @{ Name = "package.json";         Path = "package.json";              Kind = "json"  },
        @{ Name = "tauri.conf.json";      Path = "src-tauri\tauri.conf.json"; Kind = "json"  },
        @{ Name = "src-tauri/Cargo.toml"; Path = "src-tauri\Cargo.toml";      Kind = "cargo" }
    )

    $found = @()
    foreach ($t in $targets) {
        if (-not (Test-Path -LiteralPath $t.Path)) { throw "缺少文件: $($t.Path)" }
        if ($t.Kind -eq "json") { $v = Get-JsonVersion $t.Path } else { $v = Get-CargoVersion $t.Path }
        Write-Host ("    {0,-22} {1}" -f $t.Name, $v)
        $found += $v
    }

    $unique = @($found | Sort-Object -Unique)
    if ($unique.Count -ne 1) {
        Write-Err2 "版本号不一致：$($unique -join ' / ')"
        throw "请先把 package.json、tauri.conf.json、src-tauri/Cargo.toml 改成同一个版本"
    }

    $script:Version = $unique[0]
    Write-Ok "版本一致：$script:Version"
}

# ---------- 工具与前置条件 ----------
function Test-Prerequisites {
    Write-Step "检查构建环境"

    foreach ($tool in @("node", "npm", "cargo")) {
        if ($null -eq (Get-Command $tool -ErrorAction SilentlyContinue)) {
            throw "找不到 $tool，请先安装并加入 PATH"
        }
    }
    Write-Ok "node / npm / cargo 就绪"

    $tauriExe = Join-Path $repoRoot "node_modules\.bin\tauri.cmd"
    if (-not (Test-Path -LiteralPath $tauriExe)) {
        throw "找不到 $tauriExe，请先在项目根目录执行 npm install"
    }
    Write-Ok "Tauri CLI 就绪"

    $needAndroid = ($Platform -eq "android") -or ($Platform -eq "all")
    if ($needAndroid) {
        if (-not (Test-Path -LiteralPath "src-tauri\gen\android")) {
            throw "缺少 src-tauri\gen\android，请先执行: npm run android:init"
        }
        Write-Ok "Android 原生工程存在"

        if ($env:ANDROID_HOME -or $env:ANDROID_SDK_ROOT) {
            Write-Ok "Android SDK 环境变量已设置"
        } else {
            Write-Warn2 "未检测到 ANDROID_HOME / ANDROID_SDK_ROOT（构建可能失败）"
        }
        if ($env:NDK_HOME) { Write-Ok "NDK_HOME = $env:NDK_HOME" } else { Write-Warn2 "未检测到 NDK_HOME（构建可能失败）" }
        if ($env:JAVA_HOME) { Write-Ok "JAVA_HOME = $env:JAVA_HOME" } else { Write-Warn2 "未检测到 JAVA_HOME（建议指向 Android Studio 的 jbr）" }

        $keystoreProps = "src-tauri\gen\android\keystore.properties"
        if (Test-Path -LiteralPath $keystoreProps) {
            Write-Ok "已配置 Android 签名（keystore.properties）"
        } else {
            Write-Warn2 "未找到 keystore.properties：首次 build 时 Tauri 会交互式引导你配置签名"
        }
    }
}

# ---------- 调用 Tauri CLI ----------
function Invoke-Tauri {
    param([string[]]$TauriArgs)

    $exe = Join-Path $repoRoot "node_modules\.bin\tauri.cmd"
    Write-Host ("    > tauri " + ($TauriArgs -join " ")) -ForegroundColor DarkGray
    & $exe @TauriArgs
    if ($LASTEXITCODE -ne 0) { throw "tauri 命令失败（exit $LASTEXITCODE）" }
}

function Invoke-WindowsBuild {
    Write-Step "构建 Windows 安装包（$Bundle）"
    if ($Bundle -eq "nsis") {
        Invoke-Tauri @("build", "--bundles", "nsis")
    } elseif ($Bundle -eq "msi") {
        Invoke-Tauri @("build", "--bundles", "msi")
    } else {
        Invoke-Tauri @("build")
    }
    Write-Ok "Windows 构建完成"
}

function Invoke-AndroidBuild {
    Write-Step "构建 Android 安装包（$AndroidTarget）"

    # 注意：不要用 $args（PowerShell 自动变量）
    $tauriArgs = @("android", "build")
    if ($AndroidTarget -eq "apk") {
        $tauriArgs += "--apk"
    } elseif ($AndroidTarget -eq "aab") {
        $tauriArgs += "--aab"
    } else {
        $tauriArgs += "--apk"
        $tauriArgs += "--aab"
    }
    if ($Slim) {
        $tauriArgs += "-t"
        $tauriArgs += "aarch64"
    }

    Invoke-Tauri $tauriArgs
    Write-Ok "Android 构建完成"
}

# ---------- 产物 ----------
function Get-Artifacts {
    param(
        [string[]]$Roots,
        [string[]]$Files,
        [string[]]$Extensions
    )

    $result = @()
    foreach ($root in $Roots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        $items = Get-ChildItem -LiteralPath $root -Recurse -File -ErrorAction SilentlyContinue
        foreach ($f in $items) {
            if ($Extensions -notcontains $f.Extension.ToLower()) { continue }
            # 过滤掉 dev/debug 产物，只列发布包
            if ($f.Name -match "(?i)debug") { continue }
            $result += $f
        }
    }
    foreach ($p in $Files) {
        if (Test-Path -LiteralPath $p) { $result += (Get-Item -LiteralPath $p) }
    }
    return $result
}

function Show-Artifacts {
    param([object[]]$Items)

    Write-Step "构建产物"
    if ($Items.Count -eq 0) {
        Write-Warn2 "没有找到产物，请检查上面的构建输出"
        return $null
    }

    $firstDir = $null
    foreach ($f in ($Items | Sort-Object FullName)) {
        $mb = [math]::Round($f.Length / 1MB, 2)
        Write-Host ("    {0,8} MB   {1}" -f $mb, $f.FullName)
        if ($null -eq $firstDir) { $firstDir = $f.DirectoryName }
    }
    Write-Ok ("共 {0} 个文件" -f $Items.Count)
    return $firstDir
}

# ---------- 主流程 ----------
$started = Get-Date

try {
    if (-not $SkipChecks) {
        Test-VersionConsistency
        Test-Prerequisites
    }

    if ($Platform -eq "windows" -or $Platform -eq "all") { Invoke-WindowsBuild }
    if ($Platform -eq "android" -or $Platform -eq "all") { Invoke-AndroidBuild }

    $roots = @()
    $files = @()
    $exts  = @()

    if ($Platform -eq "windows" -or $Platform -eq "all") {
        $roots += "src-tauri\target\release\bundle"
        $files += "src-tauri\target\release\jlu-oa-notifier.exe"
        $exts  += @(".exe", ".msi")
    }
    if ($Platform -eq "android" -or $Platform -eq "all") {
        $roots += "src-tauri\gen\android\app\build\outputs"
        $exts  += @(".apk", ".aab")
    }

    $artifacts = @(Get-Artifacts -Roots $roots -Files $files -Extensions $exts)
    $firstDir = Show-Artifacts -Items $artifacts

    Write-Step "发布提醒"
    Write-Host "    - 版本号：$script:Version"
    Write-Host "    - Windows 未做代码签名时，用户安装会看到 SmartScreen「未知发布者」警告"
    Write-Host "    - Android 的 keystore 与密码务必离线备份，丢失后无法再出升级包"
    Write-Host "    - 本应用依赖 oa.jlu.edu.cn，需告知使用者必须在校园网 / VPN 环境下使用"

    $elapsed = (Get-Date) - $started
    Write-Host ""
    Write-Host ("完成，用时 {0:mm\:ss}" -f $elapsed) -ForegroundColor Green

    if ($Open -and $firstDir) {
        Write-Host "打开目录: $firstDir" -ForegroundColor DarkGray
        Start-Process -FilePath "explorer.exe" -ArgumentList "`"$firstDir`""
    }

    $exitCode = 0
}
catch {
    Write-Host ""
    Write-Err2 "构建中止：$($_.Exception.Message)"
    $exitCode = 1
}

exit $exitCode
