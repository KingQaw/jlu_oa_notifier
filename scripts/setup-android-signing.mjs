#!/usr/bin/env node
/**
 * 为生成的 Android 工程配置发布签名。
 *
 * 为什么需要脚本：`src-tauri/gen/` 被 gitignore，且 `npm run android:init`
 * 会重新生成整个工程，手改的 `app/build.gradle.kts` 会丢失。此脚本可重复执行，
 * init 之后跑一次即可恢复签名配置。
 *
 * 用法：
 *   node scripts/setup-android-signing.mjs
 *   node scripts/setup-android-signing.mjs --store D:/keys/jlu-oa.keystore \
 *        --alias jlu-oa --store-password 123456 --key-password 123456
 *
 * 默认从环境变量读取（推荐，避免明文留在 shell 历史里）：
 *   ANDROID_KEYSTORE_PATH / ANDROID_KEYSTORE_PASSWORD /
 *   ANDROID_KEY_ALIAS / ANDROID_KEY_PASSWORD
 */

import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const genAndroid = resolve(root, "src-tauri/gen/android");
const gradleFile = resolve(genAndroid, "app/build.gradle.kts");
const propsFile = resolve(genAndroid, "keystore.properties");

/** 解析 `--key value` 形式的参数。 */
function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i !== -1 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

const storeFile = arg("store", process.env.ANDROID_KEYSTORE_PATH);
const storePassword = arg("store-password", process.env.ANDROID_KEYSTORE_PASSWORD);
const keyAlias = arg("alias", process.env.ANDROID_KEY_ALIAS);
const keyPassword = arg("key-password", process.env.ANDROID_KEY_PASSWORD);

if (!existsSync(genAndroid)) {
  console.error(
    `找不到 ${genAndroid}\n请先执行：npm run android:init`,
  );
  process.exit(1);
}

if (!storeFile || !storePassword || !keyAlias || !keyPassword) {
  console.error(
    "缺少签名参数。请通过环境变量提供：\n" +
      "  ANDROID_KEYSTORE_PATH / ANDROID_KEYSTORE_PASSWORD / " +
      "ANDROID_KEY_ALIAS / ANDROID_KEY_PASSWORD\n" +
      "或使用 --store / --store-password / --alias / --key-password。",
  );
  process.exit(1);
}

// 1) 写 keystore.properties（gen/ 已被 gitignore，不会入库）
const props = [
  "# 由 scripts/setup-android-signing.mjs 生成，勿提交（gen/ 已在 .gitignore 中）",
  `storeFile=${storeFile.replace(/\\/g, "/")}`,
  `storePassword=${storePassword}`,
  `keyAlias=${keyAlias}`,
  `keyPassword=${keyPassword}`,
  "",
].join("\n");
writeFileSync(propsFile, props, "utf8");
console.log(`已写入 ${propsFile}`);

// 2) 在 app/build.gradle.kts 里接入签名配置（幂等）
let g = readFileSync(gradleFile, "utf8");

if (g.includes("hasReleaseKeystore")) {
  console.log("build.gradle.kts 已包含签名配置，跳过");
} else {
  const anchor = `android {\n    compileSdk = 36`;
  if (!g.includes(anchor)) {
    console.error("未找到预期的 android { } 起始位置，请手动检查 build.gradle.kts");
    process.exit(1);
  }
  const block = `// 发布签名：从 gen/android/keystore.properties 读取（该文件不入库）。
// 由 scripts/setup-android-signing.mjs 注入；android:init 之后需重新执行。
val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}
val hasReleaseKeystore = keystoreProperties.getProperty("storeFile") != null

${anchor}
    signingConfigs {
        if (hasReleaseKeystore) {
            create("release") {
                storeFile = file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
            }
        }
    }`;
  g = g.replace(anchor, block);

  const releaseAnchor = `getByName("release") {\n            isMinifyEnabled = true`;
  if (!g.includes(releaseAnchor)) {
    console.error("未找到 release buildType，请手动检查 build.gradle.kts");
    process.exit(1);
  }
  g = g.replace(
    releaseAnchor,
    `getByName("release") {\n            isMinifyEnabled = true\n            if (hasReleaseKeystore) {\n                signingConfig = signingConfigs.getByName("release")\n            }`,
  );

  writeFileSync(gradleFile, g, "utf8");
  console.log(`已更新 ${gradleFile}`);
}

console.log("\n完成。现在可以构建发行版：");
console.log("  npx tauri android build --apk --target aarch64");
console.log("\n提醒：务必另行备份 keystore 与口令，丢失后无法为已发布应用出升级包。");
