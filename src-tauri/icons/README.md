# Icons

打包需要真实图标文件（32x32.png / 128x128.png / icon.icns / icon.ico）。
可用 `pnpm tauri icon <源图>` 一键生成；在生成前，本地 `cargo tauri build` 的 bundle 步骤会跳过图标校验（开发模式 `tauri dev` 不受影响）。
